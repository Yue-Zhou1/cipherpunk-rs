use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use audit_agent_core::audit_config::{AuditConfig, BudgetConfig, ResolvedSource, SourceOrigin};
use audit_agent_core::output::AuditManifest;
use audit_agent_core::session::{AuditSession, SessionUiState};
use chrono::Utc;
use intake::config::{ConfigParser, RawEngineConfig, RawScope, RawSource, ValidatedConfig};
use intake::confirmation::{ConfirmationSummary, UserDecisions};
use intake::project_snapshot_from_config;
use intake::source::SourceInput;
use orchestrator::{AuditJob, AuditJobKind, AuditOrchestrator};
use serde::{Deserialize, Serialize};
use session_store::SessionStore;

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionJobView {
    pub job_id: String,
    pub kind: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateAuditSessionResponse {
    pub session_id: String,
    pub snapshot_id: String,
    pub initial_jobs: Vec<SessionJobView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditSessionSummary {
    pub session_id: String,
    pub snapshot_id: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenAuditSessionResponse {
    pub session_id: String,
    pub snapshot_id: String,
    pub selected_domains: Vec<String>,
    pub initial_jobs: Vec<SessionJobView>,
}

pub struct UiSessionState {
    work_dir: PathBuf,
    resolved_source: Option<ResolvedSourceView>,
    validated_config: Option<ValidatedConfig>,
    confirmation_summary: Option<ConfirmationSummary>,
    audit_config: Option<AuditConfig>,
    active_session_id: Option<String>,
    sessions: HashMap<String, AuditSession>,
    session_jobs: HashMap<String, Vec<SessionJobView>>,
    session_store: Option<Arc<SessionStore>>,
}

impl UiSessionState {
    pub fn new(work_dir: PathBuf) -> Self {
        let session_store = SessionStore::open(work_dir.join("sessions.sqlite"))
            .ok()
            .map(Arc::new);
        Self {
            work_dir,
            resolved_source: None,
            validated_config: None,
            confirmation_summary: None,
            audit_config: None,
            active_session_id: None,
            sessions: HashMap::new(),
            session_jobs: HashMap::new(),
            session_store,
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

    pub async fn create_audit_session(&mut self) -> Result<CreateAuditSessionResponse> {
        let config = self
            .audit_config
            .as_ref()
            .context("confirm_workspace must be called before create_audit_session")?
            .clone();

        let session_id = make_session_id();
        let snapshot_id = make_snapshot_id();
        let now = Utc::now();
        let session = AuditSession {
            session_id: session_id.clone(),
            snapshot: project_snapshot_from_config(&config, snapshot_id.clone()),
            selected_domains: config
                .scope
                .detected_frameworks
                .iter()
                .map(|framework| format!("{framework:?}").to_ascii_lowercase())
                .collect(),
            ui_state: SessionUiState::default(),
            created_at: now,
            updated_at: now,
        };

        if let Some(store) = &self.session_store {
            store.create_session(&session)?;
        }
        self.active_session_id = Some(session_id.clone());
        self.sessions.insert(session_id.clone(), session.clone());

        let orchestrator = session_orchestrator(&self.work_dir, self.session_store.clone());
        let jobs = orchestrator.bootstrap_jobs(&session).await?;
        let initial_jobs = jobs.iter().map(job_to_view).collect::<Vec<_>>();
        self.session_jobs
            .insert(session_id.clone(), initial_jobs.clone());

        Ok(CreateAuditSessionResponse {
            session_id,
            snapshot_id,
            initial_jobs,
        })
    }

    pub async fn create_audit_session_for_tests(&mut self) -> Result<CreateAuditSessionResponse> {
        if self.audit_config.is_none() {
            self.audit_config = Some(test_audit_config(&self.work_dir));
        }
        self.create_audit_session().await
    }

    pub fn list_audit_sessions(&self) -> Result<Vec<AuditSessionSummary>> {
        let mut by_id = self
            .sessions
            .iter()
            .map(|(session_id, session)| (session_id.clone(), session.clone()))
            .collect::<HashMap<_, _>>();
        if let Some(store) = &self.session_store {
            for session in store.list_sessions()? {
                by_id.entry(session.session_id.clone()).or_insert(session);
            }
        }

        let mut sessions = by_id
            .values()
            .map(|session| AuditSessionSummary {
                session_id: session.session_id.clone(),
                snapshot_id: session.snapshot.snapshot_id.clone(),
                created_at: session.created_at.to_rfc3339(),
                updated_at: session.updated_at.to_rfc3339(),
            })
            .collect::<Vec<_>>();
        sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(sessions)
    }

    pub async fn open_audit_session(
        &mut self,
        session_id: &str,
    ) -> Result<Option<OpenAuditSessionResponse>> {
        let session = if let Some(existing) = self.sessions.get(session_id) {
            existing.clone()
        } else if let Some(store) = &self.session_store {
            let Some(loaded) = store.load_session(session_id)? else {
                return Ok(None);
            };
            self.sessions.insert(session_id.to_string(), loaded.clone());
            loaded
        } else {
            return Ok(None);
        };

        self.active_session_id = Some(session.session_id.clone());

        let initial_jobs = if let Some(cached_jobs) = self.session_jobs.get(&session.session_id) {
            cached_jobs.clone()
        } else if let Some(store) = &self.session_store {
            let loaded = session_jobs_from_events(&store.list_events(&session.session_id)?);
            if loaded.is_empty() {
                vec![]
            } else {
                self.session_jobs
                    .insert(session.session_id.clone(), loaded.clone());
                loaded
            }
        } else {
            vec![]
        };

        Ok(Some(OpenAuditSessionResponse {
            session_id: session.session_id,
            snapshot_id: session.snapshot.snapshot_id,
            selected_domains: session.selected_domains,
            initial_jobs,
        }))
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

fn make_session_id() -> String {
    format!("sess-{}", Utc::now().timestamp_micros())
}

fn make_snapshot_id() -> String {
    format!("snap-{}", Utc::now().timestamp_micros())
}

fn job_to_view(job: &AuditJob) -> SessionJobView {
    SessionJobView {
        job_id: job.job_id.clone(),
        kind: job_kind(&job.kind),
        status: format!("{:?}", job.status).to_ascii_lowercase(),
    }
}

fn job_kind(kind: &AuditJobKind) -> String {
    match kind {
        AuditJobKind::BuildProjectIr => "build_project_ir".to_string(),
        AuditJobKind::GenerateAiOverview => "generate_ai_overview".to_string(),
        AuditJobKind::PlanChecklists => "plan_checklists".to_string(),
        AuditJobKind::RunDomainChecklist { domain_id } => {
            format!("run_domain_checklist:{domain_id}")
        }
        AuditJobKind::RunToolAction { action_id } => format!("run_tool_action:{action_id}"),
        AuditJobKind::ExportReports => "export_reports".to_string(),
    }
}

fn session_jobs_from_events(events: &[session_store::SessionEvent]) -> Vec<SessionJobView> {
    let mut seen = HashMap::<String, ()>::new();
    let mut jobs = Vec::<SessionJobView>::new();
    for event in events {
        let Ok(job) = serde_json::from_str::<AuditJob>(&event.payload) else {
            continue;
        };
        if seen.insert(job.job_id.clone(), ()).is_none() {
            jobs.push(job_to_view(&job));
        }
    }
    jobs
}

fn test_audit_config(work_dir: &Path) -> AuditConfig {
    AuditConfig {
        audit_id: format!("audit-test-{}", Utc::now().timestamp()),
        source: ResolvedSource {
            local_path: work_dir.to_path_buf(),
            origin: SourceOrigin::Local {
                original_path: work_dir.to_path_buf(),
            },
            commit_hash: "0000000000000000000000000000000000000000".to_string(),
            content_hash: "sha256:test".to_string(),
        },
        scope: audit_agent_core::audit_config::ResolvedScope {
            target_crates: vec![],
            excluded_crates: vec![],
            build_matrix: vec![],
            detected_frameworks: vec![],
        },
        engines: audit_agent_core::audit_config::EngineConfig {
            crypto_zk: true,
            distributed: false,
        },
        budget: BudgetConfig {
            kani_timeout_secs: 300,
            z3_timeout_secs: 600,
            fuzz_duration_secs: 3600,
            madsim_ticks: 100_000,
            max_llm_retries: 3,
            semantic_index_timeout_secs: 120,
        },
        optional_inputs: audit_agent_core::audit_config::OptionalInputs {
            spec_document: None,
            previous_audit: None,
            custom_invariants: vec![],
            known_entry_points: vec![],
        },
        llm: audit_agent_core::audit_config::LlmConfig {
            api_key_present: false,
            provider: None,
            no_llm_prose: false,
        },
        output_dir: PathBuf::from("audit-output"),
    }
}

fn session_orchestrator(
    work_dir: &Path,
    session_store: Option<Arc<SessionStore>>,
) -> AuditOrchestrator {
    let mut orchestrator = AuditOrchestrator::new(
        work_dir.join("audit-output"),
        work_dir.join("evidence-pack.zip"),
    );
    if let Some(store) = session_store {
        orchestrator = orchestrator.with_session_store(store);
    }
    orchestrator
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
        SourceOrigin::Archive {
            original_filename: _,
        } => RawSource {
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
