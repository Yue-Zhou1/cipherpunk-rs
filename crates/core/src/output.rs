use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::audit_config::{OptionalInputsSummary, ResolvedScope, ResolvedSource};
use crate::finding::{Finding, Severity};
use crate::session::AuditRecord;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AuditOutputs {
    pub dir: PathBuf,
    pub manifest: AuditManifest,
    pub findings: Vec<Finding>,
    #[serde(default)]
    pub candidates: Vec<AuditRecord>,
    #[serde(default)]
    pub review_notes: Vec<AuditRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AuditManifest {
    pub audit_id: String,
    pub agent_version: String,
    pub source: ResolvedSource,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub scope: ResolvedScope,
    pub tool_versions: HashMap<String, String>,
    pub container_digests: HashMap<String, String>,
    pub finding_counts: FindingCounts,
    pub risk_score: u8,
    pub engines_run: Vec<String>,
    pub optional_inputs_used: OptionalInputsSummary,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct FindingCounts {
    pub critical: u32,
    pub high: u32,
    pub medium: u32,
    pub low: u32,
    pub observation: u32,
}

impl FindingCounts {
    pub fn from(findings: &[Finding]) -> Self {
        findings.iter().fold(Self::default(), |mut acc, finding| {
            match finding.severity {
                Severity::Critical => acc.critical += 1,
                Severity::High => acc.high += 1,
                Severity::Medium => acc.medium += 1,
                Severity::Low => acc.low += 1,
                Severity::Observation => acc.observation += 1,
            }
            acc
        })
    }

    pub fn risk_score(&self) -> u8 {
        let raw = 100i32
            - (self.critical as i32 * 25)
            - (self.high as i32 * 15)
            - (self.medium as i32 * 5)
            - (self.low as i32 * 2);
        raw.clamp(0, 100) as u8
    }
}
