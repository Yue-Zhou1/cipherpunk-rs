use audit_agent_core::finding::VerificationStatus;
use audit_agent_core::session::{
    AuditRecord, AuditRecordKind, AuditSession, ProjectSnapshot, SessionUiState,
};

#[test]
fn audit_record_kind_round_trips_through_json() {
    let record = AuditRecord::candidate(
        "CAND-001",
        "Possible nonce reuse",
        VerificationStatus::Unverified {
            reason: "AI hotspot review".to_string(),
        },
    );
    let json = serde_json::to_string(&record).expect("serialize record");
    let parsed: AuditRecord = serde_json::from_str(&json).expect("deserialize record");
    assert_eq!(parsed.kind, AuditRecordKind::Candidate);
}

#[test]
fn audit_session_embeds_snapshot_domains_and_ui_state() {
    let session = AuditSession {
        session_id: "sess-1".to_string(),
        snapshot: ProjectSnapshot::minimal("snap-1"),
        selected_domains: vec!["crypto".to_string(), "consensus".to_string()],
        ui_state: SessionUiState::default(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    assert_eq!(session.selected_domains.len(), 2);
}
