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
    #[serde(default)]
    pub engine_outcomes: Vec<EngineOutcome>,
    #[serde(default)]
    pub coverage: Option<CoverageReport>,
    pub optional_inputs_used: OptionalInputsSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct EngineOutcome {
    pub engine: String,
    pub status: EngineStatus,
    pub findings_count: usize,
    pub duration_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adviser_suggestion: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum EngineStatus {
    Completed,
    Failed { reason: String },
    Skipped { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CoverageReport {
    pub engines_requested: usize,
    pub engines_completed: usize,
    pub engines_failed: usize,
    pub engines_skipped: usize,
    pub coverage_complete: bool,
    pub warnings: Vec<String>,
    #[serde(default)]
    pub failover_warnings: Vec<String>,
}

impl CoverageReport {
    pub fn from_outcomes(outcomes: &[EngineOutcome]) -> Self {
        let completed = outcomes
            .iter()
            .filter(|outcome| matches!(outcome.status, EngineStatus::Completed))
            .count();
        let failed = outcomes
            .iter()
            .filter(|outcome| matches!(outcome.status, EngineStatus::Failed { .. }))
            .count();
        let skipped = outcomes
            .iter()
            .filter(|outcome| matches!(outcome.status, EngineStatus::Skipped { .. }))
            .count();
        let mut warnings = Vec::new();

        for outcome in outcomes {
            if let EngineStatus::Failed { reason } = &outcome.status {
                warnings.push(format!("Engine '{}' failed: {}", outcome.engine, reason));
            }
            if let EngineStatus::Skipped { reason } = &outcome.status {
                warnings.push(format!("Engine '{}' skipped: {}", outcome.engine, reason));
            }
        }

        Self {
            engines_requested: outcomes.len(),
            engines_completed: completed,
            engines_failed: failed,
            engines_skipped: skipped,
            coverage_complete: failed == 0 && skipped == 0,
            warnings,
            failover_warnings: vec![],
        }
    }

    pub fn confidence_percent(&self) -> u8 {
        if self.engines_requested == 0 {
            return 0;
        }
        ((self.engines_completed * 100) / self.engines_requested) as u8
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coverage_report_marks_partial_coverage_and_collects_warnings() {
        let outcomes = vec![
            EngineOutcome {
                engine: "engine-ok".to_string(),
                status: EngineStatus::Completed,
                findings_count: 1,
                duration_ms: 10,
                adviser_suggestion: None,
            },
            EngineOutcome {
                engine: "engine-failed".to_string(),
                status: EngineStatus::Failed {
                    reason: "boom".to_string(),
                },
                findings_count: 0,
                duration_ms: 20,
                adviser_suggestion: None,
            },
            EngineOutcome {
                engine: "engine-skipped".to_string(),
                status: EngineStatus::Skipped {
                    reason: "unsupported".to_string(),
                },
                findings_count: 0,
                duration_ms: 0,
                adviser_suggestion: None,
            },
        ];

        let report = CoverageReport::from_outcomes(&outcomes);

        assert_eq!(report.engines_requested, 3);
        assert_eq!(report.engines_completed, 1);
        assert_eq!(report.engines_failed, 1);
        assert_eq!(report.engines_skipped, 1);
        assert!(!report.coverage_complete);
        assert_eq!(report.warnings.len(), 2);
        assert!(report.warnings.iter().any(|msg| msg.contains("failed")));
        assert!(report.warnings.iter().any(|msg| msg.contains("skipped")));
        assert!(report.failover_warnings.is_empty());
    }

    #[test]
    fn confidence_percent_reflects_completed_coverage_only() {
        let full = CoverageReport {
            engines_requested: 2,
            engines_completed: 2,
            engines_failed: 0,
            engines_skipped: 0,
            coverage_complete: true,
            warnings: vec![],
            failover_warnings: vec![],
        };
        assert_eq!(full.confidence_percent(), 100);

        let partial = CoverageReport {
            engines_requested: 4,
            engines_completed: 2,
            engines_failed: 1,
            engines_skipped: 1,
            coverage_complete: false,
            warnings: vec![],
            failover_warnings: vec![],
        };
        assert_eq!(partial.confidence_percent(), 50);

        let none_requested = CoverageReport {
            engines_requested: 0,
            engines_completed: 0,
            engines_failed: 0,
            engines_skipped: 0,
            coverage_complete: false,
            warnings: vec![],
            failover_warnings: vec![],
        };
        assert_eq!(none_requested.confidence_percent(), 0);
    }

    #[test]
    fn finding_risk_score_is_independent_from_coverage_confidence() {
        let findings = FindingCounts {
            critical: 0,
            high: 1,
            medium: 0,
            low: 0,
            observation: 0,
        };
        assert_eq!(findings.risk_score(), 85);

        let low_confidence = CoverageReport {
            engines_requested: 10,
            engines_completed: 1,
            engines_failed: 9,
            engines_skipped: 0,
            coverage_complete: false,
            warnings: vec![],
            failover_warnings: vec![],
        };
        assert_eq!(low_confidence.confidence_percent(), 10);
        assert_eq!(findings.risk_score(), 85);
    }

    #[test]
    fn failover_warnings_are_retained_separately() {
        let mut report = CoverageReport::from_outcomes(&[]);
        report.failover_warnings = vec![
            "LLM provider failover occurred: Scaffolding switched from openai to template-fallback."
                .to_string(),
        ];
        report.warnings.extend(report.failover_warnings.clone());

        assert_eq!(report.failover_warnings.len(), 1);
        assert_eq!(report.warnings.len(), 1);
        assert!(report.warnings[0].contains("failover"));
    }
}
