use std::path::PathBuf;

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::audit_config::{BuildVariant, ResolvedSource, SourceOrigin};
use crate::finding::{CodeLocation, Framework, Severity, VerificationStatus};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum AuditRecordKind {
    ReviewNote,
    Candidate,
    Finding,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ProjectSnapshot {
    pub snapshot_id: String,
    pub source: ResolvedSource,
    pub target_crates: Vec<String>,
    pub excluded_crates: Vec<String>,
    pub build_matrix: Vec<BuildVariant>,
    pub detected_frameworks: Vec<Framework>,
}

impl ProjectSnapshot {
    pub fn minimal(snapshot_id: impl Into<String>) -> Self {
        Self {
            snapshot_id: snapshot_id.into(),
            source: ResolvedSource {
                local_path: PathBuf::new(),
                origin: SourceOrigin::Local {
                    original_path: PathBuf::new(),
                },
                commit_hash: String::new(),
                content_hash: String::new(),
            },
            target_crates: vec![],
            excluded_crates: vec![],
            build_matrix: vec![],
            detected_frameworks: vec![],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AuditRecord {
    pub record_id: String,
    pub kind: AuditRecordKind,
    pub title: String,
    pub summary: String,
    pub severity: Option<Severity>,
    pub verification_status: VerificationStatus,
    pub locations: Vec<CodeLocation>,
    pub evidence_refs: Vec<String>,
    pub labels: Vec<String>,
    #[serde(default)]
    pub ir_node_ids: Vec<String>,
}

impl AuditRecord {
    pub fn candidate(
        record_id: impl Into<String>,
        title: impl Into<String>,
        verification_status: VerificationStatus,
    ) -> Self {
        let title = title.into();
        Self {
            record_id: record_id.into(),
            kind: AuditRecordKind::Candidate,
            summary: title.clone(),
            title,
            severity: None,
            verification_status,
            locations: vec![],
            evidence_refs: vec![],
            labels: vec![],
            ir_node_ids: vec![],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AuditSession {
    pub session_id: String,
    pub snapshot: ProjectSnapshot,
    pub selected_domains: Vec<String>,
    pub ui_state: SessionUiState,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl AuditSession {
    pub fn sample(session_id: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            session_id: session_id.into(),
            snapshot: ProjectSnapshot::minimal("snapshot-default"),
            selected_domains: vec![],
            ui_state: SessionUiState::default(),
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SessionUiState {
    pub active_file: Option<PathBuf>,
    pub active_record_id: Option<String>,
    pub active_graph_view: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::finding::VerificationStatus;

    #[test]
    fn audit_record_serde_roundtrip_preserves_ir_node_ids() {
        let mut record = AuditRecord::candidate(
            "cand-1",
            "candidate finding",
            VerificationStatus::unverified("pending"),
        );
        record.ir_node_ids = vec![
            "file:/tmp/repo/src/lib.rs".to_string(),
            "symbol:/tmp/repo/src/lib.rs::aead_encrypt".to_string(),
        ];

        let encoded = serde_json::to_string(&record).expect("serialize record");
        let decoded: AuditRecord = serde_json::from_str(&encoded).expect("deserialize record");

        assert_eq!(decoded.ir_node_ids, record.ir_node_ids);
    }

    #[test]
    fn audit_record_serde_defaults_ir_node_ids_for_legacy_json() {
        let legacy_json = serde_json::json!({
            "record_id": "cand-legacy",
            "kind": "Candidate",
            "title": "legacy",
            "summary": "legacy record",
            "severity": null,
            "verification_status": {"Unverified": {"reason": "legacy"}},
            "locations": [],
            "evidence_refs": [],
            "labels": []
        });

        let decoded: AuditRecord = serde_json::from_value(legacy_json).expect("deserialize");
        assert!(
            decoded.ir_node_ids.is_empty(),
            "legacy records should deserialize with empty provenance"
        );
    }
}
