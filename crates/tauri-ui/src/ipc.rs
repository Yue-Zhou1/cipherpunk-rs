use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use audit_agent_core::audit_config::{AuditConfig, BudgetConfig, ResolvedSource, SourceOrigin};
use audit_agent_core::output::AuditManifest;
use intake::config::{
    ConfigParser, RawEngineConfig, RawScope, RawSource, ValidatedConfig,
};
use intake::confirmation::{ConfirmationSummary, UserDecisions};
use intake::source::SourceInput;
use serde::{Deserialize, Serialize};

use crate::{
    ConfigParseResponse, OutputType, ResolvedSourceView, confirm_workspace, detect_workspace,
    download_output, export_audit_yaml, get_audit_manifest, resolve_source,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceInputIpc {
    pub kind: SourceKind,
    pub value: String,
    pub commit_or_ref: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceKind {
    Git,
    Local,
    Archive,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmWorkspaceRequest {
    pub confirmed: bool,
    pub ambiguous_crates: HashMap<String, bool>,
    #[serde(default)]
    pub no_llm_prose: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmWorkspaceResponse {
    pub audit_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadOutputResponse {
    pub dest: PathBuf,
}

#[derive(Debug, Clone)]
pub struct UiSessionState {
    work_dir: PathBuf,
    resolved_source: Option<ResolvedSourceView>,
    validated_config: Option<ValidatedConfig>,
    confirmation_summary: Option<ConfirmationSummary>,
    audit_config: Option<AuditConfig>,
}

impl UiSessionState {
    pub fn new(work_dir: PathBuf) -> Self {
        Self {
            work_dir,
            resolved_source: None,
            validated_config: None,
            confirmation_summary: None,
            audit_config: None,
        }
    }

    pub fn resolved_source(&self) -> Option<&ResolvedSourceView> {
        self.resolved_source.as_ref()
    }

    pub fn set_resolved_source(&mut self, resolved_source: ResolvedSourceView) {
        self.resolved_source = Some(resolved_source);
        self.confirmation_summary = None;
        self.audit_config = None;
    }

    pub fn set_validated_config(&mut self, validated_config: ValidatedConfig) {
        self.validated_config = Some(validated_config);
    }

    pub fn set_confirmation_summary(&mut self, summary: ConfirmationSummary) {
        self.confirmation_summary = Some(summary);
    }

    pub fn audit_config(&self) -> Option<&AuditConfig> {
        self.audit_config.as_ref()
    }

    pub async fn resolve_source(&mut self, input: SourceInputIpc) -> Result<ResolvedSourceView> {
        let source_input = input.into_source_input()?;
        let resolved = resolve_source(source_input, &self.work_dir).await?;
        self.resolved_source = Some(resolved.clone());
        self.confirmation_summary = None;
        self.audit_config = None;
        Ok(resolved)
    }

    pub fn parse_config(&mut self, path: &Path) -> ConfigParseResponse {
        match ConfigParser::parse(path) {
            Ok(validated) => {
                let response = ConfigParseResponse::Validated {
                    target_crates: validated.scope.target_crates.clone(),
                    exclude_crates: validated.scope.exclude_crates.clone(),
                    output_dir: validated.output_dir.clone(),
                };
                self.validated_config = Some(validated);
                response
            }
            Err(errors) => ConfigParseResponse::ConfigErrors {
                errors: errors.into_iter().map(|error| format!("{error}")).collect(),
            },
        }
    }

    pub fn detect_workspace(&mut self) -> Result<ConfirmationSummary> {
        let source = self
            .resolved_source
            .as_ref()
            .context("resolve_source must be called before detect_workspace")?;
        let summary = detect_workspace(&source.source)?;
        self.confirmation_summary = Some(summary.clone());
        Ok(summary)
    }

    pub fn confirm_workspace(
        &mut self,
        request: ConfirmWorkspaceRequest,
    ) -> Result<ConfirmWorkspaceResponse> {
        let source = self
            .resolved_source
            .as_ref()
            .context("resolve_source must be called before confirm_workspace")?;

        let summary = match self.confirmation_summary.clone() {
            Some(summary) => summary,
            None => {
                let summary = detect_workspace(&source.source)?;
                self.confirmation_summary = Some(summary.clone());
                summary
            }
        };

        let validated = self
            .validated_config
            .clone()
            .unwrap_or_else(|| default_validated_config(&source.source));

        let decisions = UserDecisions {
            ambiguous_crates: request.ambiguous_crates,
            override_features: None,
            confirmed: request.confirmed,
            export_audit_yaml: false,
        };

        let config = confirm_workspace(
            decisions,
            source.source.clone(),
            validated,
            summary,
            request.no_llm_prose,
        )?;
        let response = ConfirmWorkspaceResponse {
            audit_id: config.audit_id.clone(),
        };

        self.audit_config = Some(config);
        Ok(response)
    }

    pub fn export_audit_yaml(&self, path: &Path) -> Result<()> {
        let config = self
            .audit_config
            .as_ref()
            .context("confirm_workspace must be called before export_audit_yaml")?;
        export_audit_yaml(config, path)
    }

    pub fn get_audit_manifest(&self) -> Result<AuditManifest> {
        let config = self
            .audit_config
            .as_ref()
            .context("confirm_workspace must be called before get_audit_manifest")?;
        get_audit_manifest(&config.output_dir)
    }

    pub fn download_output(
        &self,
        audit_id: &str,
        output_type: OutputType,
        dest: &Path,
    ) -> Result<DownloadOutputResponse> {
        let config = self
            .audit_config
            .as_ref()
            .context("confirm_workspace must be called before download_output")?;

        if config.audit_id != audit_id {
            bail!(
                "requested audit_id `{audit_id}` does not match active audit `{}`",
                config.audit_id
            );
        }

        download_output(&config.output_dir, output_type, dest)?;
        Ok(DownloadOutputResponse {
            dest: dest.to_path_buf(),
        })
    }
}

impl SourceInputIpc {
    fn into_source_input(self) -> Result<SourceInput> {
        match self.kind {
            SourceKind::Git => {
                let commit = self
                    .commit_or_ref
                    .filter(|value| !value.trim().is_empty())
                    .context("git source requires commitOrRef")?;
                Ok(SourceInput::GitUrl {
                    url: self.value,
                    commit,
                    auth: None,
                    allow_branch_resolution: true,
                })
            }
            SourceKind::Local => Ok(SourceInput::LocalPath {
                path: PathBuf::from(self.value),
                commit: self.commit_or_ref,
            }),
            SourceKind::Archive => Ok(SourceInput::Archive {
                path: PathBuf::from(self.value),
            }),
        }
    }
}

fn default_validated_config(source: &ResolvedSource) -> ValidatedConfig {
    let source = match &source.origin {
        SourceOrigin::Git {
            url,
            original_ref: _,
        } => RawSource {
            url: Some(url.clone()),
            local_path: None,
            commit: Some(source.commit_hash.clone()),
        },
        SourceOrigin::Local { original_path } => RawSource {
            url: None,
            local_path: Some(original_path.display().to_string()),
            commit: Some(source.commit_hash.clone()),
        },
        SourceOrigin::Archive { original_filename: _ } => RawSource {
            url: None,
            local_path: Some(source.local_path.display().to_string()),
            commit: Some(source.commit_hash.clone()),
        },
    };

    ValidatedConfig {
        source,
        scope: RawScope {
            target_crates: None,
            exclude_crates: None,
            features: None,
        },
        engines: RawEngineConfig {
            crypto_zk: Some(true),
            distributed: Some(false),
        },
        budget: BudgetConfig {
            kani_timeout_secs: 300,
            z3_timeout_secs: 600,
            fuzz_duration_secs: 3600,
            madsim_ticks: 100_000,
            max_llm_retries: 3,
            semantic_index_timeout_secs: 120,
        },
        output_dir: PathBuf::from("audit-output"),
    }
}
