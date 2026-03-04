use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Result;
use audit_agent_core::audit_config::BuildVariant;
use audit_agent_core::workspace::CrateMeta;
use serde::{Deserialize, Serialize};

use crate::detection::{CryptoDivergentFeature, DetectedFramework};
use crate::source::SourceWarning;

pub struct WorkspaceConfirmation;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfirmationSummary {
    pub crates: Vec<CrateDecision>,
    pub frameworks: Vec<DetectedFramework>,
    pub crypto_divergent_features: Vec<CryptoDivergentFeature>,
    pub build_matrix: Vec<BuildVariant>,
    pub estimated_duration_mins: u64,
    pub warnings: Vec<IntakeWarning>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CrateDecision {
    InScope { meta: CrateMeta },
    Excluded { meta: CrateMeta, reason: String },
    Ambiguous { meta: CrateMeta, suggestion: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IntakeWarning {
    BranchResolved {
        branch: String,
        resolved_sha: String,
    },
    DirtyWorkingTree {
        uncommitted_files: Vec<PathBuf>,
    },
    LlmKeyMissing {
        degraded_features: Vec<String>,
    },
    LargeBuildMatrix {
        variants: usize,
        estimated_hours: f32,
    },
    PreviousAuditParsed {
        prior_finding_count: usize,
    },
}

impl From<SourceWarning> for IntakeWarning {
    fn from(value: SourceWarning) -> Self {
        match value {
            SourceWarning::BranchResolved {
                branch,
                resolved_sha,
            } => IntakeWarning::BranchResolved {
                branch,
                resolved_sha,
            },
            SourceWarning::DirtyWorkingTree { uncommitted_files } => {
                IntakeWarning::DirtyWorkingTree { uncommitted_files }
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserDecisions {
    pub ambiguous_crates: HashMap<String, bool>,
    pub override_features: Option<Vec<Vec<String>>>,
    pub confirmed: bool,
    pub export_audit_yaml: bool,
}

impl WorkspaceConfirmation {
    pub fn confirm_cli(summary: &ConfirmationSummary) -> Result<UserDecisions> {
        let ambiguous_crates = summary
            .crates
            .iter()
            .filter_map(|decision| match decision {
                CrateDecision::Ambiguous { meta, .. } => Some((meta.name.clone(), true)),
                _ => None,
            })
            .collect();

        Ok(UserDecisions {
            ambiguous_crates,
            override_features: None,
            confirmed: true,
            export_audit_yaml: false,
        })
    }

    pub fn to_json(summary: &ConfirmationSummary) -> Result<String> {
        Ok(serde_json::to_string_pretty(summary)?)
    }
}
