pub mod config;
pub mod confirmation;
pub mod detection;
pub mod optional_inputs;
pub mod source;
pub mod workspace;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use audit_agent_core::audit_config::{
    AuditConfig, BuildVariant, EngineConfig, LlmConfig, OptionalInputs, OptionalInputsSummary,
    ResolvedScope,
};

use crate::config::ConfigParser;
use crate::confirmation::{
    ConfirmationSummary, CrateDecision, IntakeWarning, UserDecisions, WorkspaceConfirmation,
};
use crate::detection::{DetectedFramework, FrameworkDetector};
use crate::optional_inputs::OptionalInputParser;
use crate::source::{SourceInput, SourceResolver};
use crate::workspace::WorkspaceAnalyzer;

pub struct IntakeOrchestrator;

#[derive(Debug, Clone, Default)]
pub struct OptionalInputsRaw {
    pub spec_path: Option<PathBuf>,
    pub previous_audit_path: Option<PathBuf>,
    pub invariants_path: Option<PathBuf>,
    pub entry_points_path: Option<PathBuf>,
}

pub struct IntakeResult {
    pub config: AuditConfig,
    pub summary: ConfirmationSummary,
    pub warnings: Vec<IntakeWarning>,
}

impl IntakeOrchestrator {
    pub async fn run(
        source: SourceInput,
        audit_yaml: &Path,
        optional: OptionalInputsRaw,
        work_dir: &Path,
    ) -> Result<IntakeResult> {
        let resolved_source = SourceResolver::resolve(&source, work_dir).await?;
        let validated = ConfigParser::parse(audit_yaml)
            .map_err(|errs| anyhow::anyhow!("config validation failed: {errs:?}"))?;

        let workspace = WorkspaceAnalyzer::analyze(&resolved_source.source.local_path)?;
        let detection = FrameworkDetector::detect(&workspace);

        let optional_inputs = OptionalInputs {
            spec_document: if let Some(path) = optional.spec_path {
                Some(OptionalInputParser::parse_spec(&path).await?)
            } else {
                None
            },
            previous_audit: if let Some(path) = optional.previous_audit_path {
                Some(OptionalInputParser::parse_previous_audit(&path).await?)
            } else {
                None
            },
            custom_invariants: if let Some(path) = optional.invariants_path {
                OptionalInputParser::parse_invariants(&path)?
            } else {
                vec![]
            },
            known_entry_points: if let Some(path) = optional.entry_points_path {
                OptionalInputParser::parse_entry_points(&path)?
            } else {
                vec![]
            },
        };

        let suggestions = WorkspaceAnalyzer::suggest_exclusions(&workspace);
        let target_crates = validated
            .scope
            .target_crates
            .unwrap_or_else(|| workspace.members.iter().map(|m| m.name.clone()).collect());
        let mut excluded_crates = validated.scope.exclude_crates.unwrap_or_default();
        for suggestion in suggestions {
            if !excluded_crates.contains(&suggestion.crate_name) {
                excluded_crates.push(suggestion.crate_name);
            }
        }

        let build_matrix = build_matrix(validated.scope.features.clone());
        let summary = ConfirmationSummary {
            crates: workspace
                .members
                .iter()
                .map(|meta| {
                    if excluded_crates.contains(&meta.name) {
                        CrateDecision::Excluded {
                            meta: meta.clone(),
                            reason: "auto-excluded by scope rules".to_string(),
                        }
                    } else {
                        CrateDecision::InScope { meta: meta.clone() }
                    }
                })
                .collect(),
            frameworks: detection.frameworks.clone(),
            crypto_divergent_features: detection.crypto_divergent_features.clone(),
            build_matrix: build_matrix.clone(),
            estimated_duration_mins: estimate_duration(target_crates.len(), build_matrix.len()),
            warnings: vec![],
        };

        // Non-interactive default for library use; CLI wrapper can call confirm_cli explicitly.
        let decisions = UserDecisions {
            ambiguous_crates: HashMap::new(),
            override_features: None,
            confirmed: true,
            export_audit_yaml: false,
        };
        if !decisions.confirmed {
            return Err(anyhow::anyhow!("workspace confirmation declined"));
        }

        let llm_missing = std::env::var("LLM_API_KEY").is_err();

        let mut warnings: Vec<IntakeWarning> = resolved_source
            .warnings
            .into_iter()
            .map(|warn| warn.into())
            .collect();
        if llm_missing {
            warnings.push(IntakeWarning::LlmKeyMissing {
                degraded_features: vec![
                    "Spec normalization".to_string(),
                    "Prose rendering".to_string(),
                ],
            });
        }

        let final_config = AuditConfig {
            audit_id: format!(
                "audit-{}-{}",
                chrono::Utc::now().format("%Y%m%d"),
                resolved_source
                    .source
                    .commit_hash
                    .chars()
                    .take(8)
                    .collect::<String>()
            ),
            source: resolved_source.source,
            scope: ResolvedScope {
                target_crates,
                excluded_crates,
                build_matrix,
                detected_frameworks: detection
                    .frameworks
                    .iter()
                    .map(|f| f.framework.clone())
                    .collect(),
            },
            engines: EngineConfig {
                crypto_zk: validated.engines.crypto_zk.unwrap_or(true),
                distributed: validated.engines.distributed.unwrap_or(false),
            },
            budget: validated.budget,
            optional_inputs,
            llm: LlmConfig {
                api_key_present: !llm_missing,
                provider: std::env::var("LLM_PROVIDER").ok(),
                no_llm_prose: false,
            },
            output_dir: validated.output_dir,
        };

        let mut summary = summary;
        summary.warnings.extend(warnings.clone());

        Ok(IntakeResult {
            config: final_config,
            summary,
            warnings,
        })
    }
}

fn build_matrix(features: Option<Vec<Vec<String>>>) -> Vec<BuildVariant> {
    let variants = features.unwrap_or_else(|| vec![vec!["default".to_string()]]);
    variants
        .into_iter()
        .map(|features| BuildVariant {
            label: if features.is_empty() {
                "default".to_string()
            } else {
                features.join(" + ")
            },
            features,
            target_triple: "x86_64-unknown-linux-gnu".to_string(),
        })
        .collect()
}

fn estimate_duration(in_scope_crates: usize, variants: usize) -> u64 {
    (in_scope_crates as u64 * 8) + (variants as u64 * 15)
}

pub fn summarize_optional_inputs(config: &AuditConfig) -> OptionalInputsSummary {
    OptionalInputsSummary {
        spec_provided: config.optional_inputs.spec_document.is_some(),
        prev_audit_provided: config.optional_inputs.previous_audit.is_some(),
        invariants_count: config.optional_inputs.custom_invariants.len(),
        entry_points_count: config.optional_inputs.known_entry_points.len(),
        llm_prose_used: config.llm.api_key_present && !config.llm.no_llm_prose,
    }
}

#[allow(dead_code)]
fn _frameworks(_frameworks: &[DetectedFramework]) {}

#[allow(dead_code)]
fn _confirm(summary: &ConfirmationSummary) -> Result<UserDecisions> {
    WorkspaceConfirmation::confirm_cli(summary)
}
