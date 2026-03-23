use audit_agent_core::finding::VerificationStatus;
use audit_agent_core::session::{AuditRecord, AuditSession};
use chrono::Utc;
use rusqlite::Connection;
use session_store::{LlmInteractionEvent, SessionEvent, SessionStore};

#[test]
fn create_and_reload_session_round_trips() {
    let dir = tempfile::tempdir().expect("temp dir");
    let store = SessionStore::open(dir.path().join("sessions.sqlite")).expect("open store");
    let session = AuditSession::sample("sess-1");
    store.create_session(&session).expect("create session");
    let loaded = store
        .load_session("sess-1")
        .expect("load session result")
        .expect("session exists");
    assert_eq!(loaded.session_id, "sess-1");
}

#[test]
fn full_text_search_returns_matching_records() {
    let dir = tempfile::tempdir().expect("temp dir");
    let store = SessionStore::open(dir.path().join("sessions.sqlite")).expect("open store");
    store
        .create_session(&AuditSession::sample("sess-1"))
        .expect("create session");
    store
        .upsert_record(
            "sess-1",
            &AuditRecord::candidate(
                "CAND-1",
                "nonce reuse in signer",
                VerificationStatus::unverified("test"),
            ),
        )
        .expect("upsert record");
    let hits = store.search_records("nonce").expect("search records");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].record_id, "CAND-1");
}

#[test]
fn persisted_sessions_can_be_loaded_after_reopening_store() {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("sessions.sqlite");

    {
        let store = SessionStore::open(&db_path).expect("open store");
        store
            .create_session(&AuditSession::sample("sess-2"))
            .expect("create session");
    }

    let reopened = SessionStore::open(&db_path).expect("reopen store");
    let loaded = reopened
        .load_session("sess-2")
        .expect("load session result")
        .expect("session exists after reopen");
    assert_eq!(loaded.session_id, "sess-2");
}

#[test]
fn list_sessions_and_events_return_persisted_data() {
    let dir = tempfile::tempdir().expect("temp dir");
    let store = SessionStore::open(dir.path().join("sessions.sqlite")).expect("open store");
    let first = AuditSession::sample("sess-a");
    let second = AuditSession::sample("sess-b");
    store.create_session(&first).expect("create first session");
    store
        .create_session(&second)
        .expect("create second session");

    store
        .append_event(
            "sess-a",
            &SessionEvent {
                event_id: "evt-1".to_string(),
                event_type: "job.lifecycle".to_string(),
                payload: r#"{"job_id":"job-1"}"#.to_string(),
                created_at: Utc::now(),
            },
        )
        .expect("append event");

    let sessions = store.list_sessions().expect("list sessions");
    assert!(
        sessions
            .iter()
            .any(|session| session.session_id == "sess-a")
    );
    assert!(
        sessions
            .iter()
            .any(|session| session.session_id == "sess-b")
    );

    let events = store.list_events("sess-a").expect("list events");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_id, "evt-1");
}

#[test]
fn open_enables_wal_journal_mode() {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("sessions.sqlite");

    let _store = SessionStore::open(&db_path).expect("open store");
    let conn = Connection::open(&db_path).expect("open sqlite for pragma check");
    let mode: String = conn
        .pragma_query_value(None, "journal_mode", |row| row.get(0))
        .expect("query journal mode");
    assert_eq!(mode.to_ascii_lowercase(), "wal");
}

#[test]
fn upsert_and_load_record_roundtrip_preserves_ir_node_ids() {
    let dir = tempfile::tempdir().expect("temp dir");
    let store = SessionStore::open(dir.path().join("sessions.sqlite")).expect("open store");
    store
        .create_session(&AuditSession::sample("sess-3"))
        .expect("create session");

    let mut record = AuditRecord::candidate(
        "cand-provenance",
        "candidate",
        VerificationStatus::unverified("x"),
    );
    record.ir_node_ids = vec![
        "file:/tmp/repo/src/lib.rs".to_string(),
        "symbol:/tmp/repo/src/lib.rs::aead_encrypt".to_string(),
    ];
    store
        .upsert_record("sess-3", &record)
        .expect("upsert candidate");

    let loaded = store
        .load_record("sess-3", "cand-provenance")
        .expect("load result")
        .expect("record exists");
    assert_eq!(loaded.ir_node_ids, record.ir_node_ids);
}

#[test]
fn legacy_record_json_without_ir_node_ids_deserializes_as_empty_vec() {
    let dir = tempfile::tempdir().expect("temp dir");
    let store = SessionStore::open(dir.path().join("sessions.sqlite")).expect("open store");
    store
        .create_session(&AuditSession::sample("sess-legacy"))
        .expect("create session");

    let mut legacy = serde_json::to_value(AuditRecord::candidate(
        "cand-legacy",
        "legacy candidate",
        VerificationStatus::unverified("legacy"),
    ))
    .expect("serialize candidate");
    legacy
        .as_object_mut()
        .expect("object value")
        .remove("ir_node_ids");

    let conn = Connection::open(store.db_path()).expect("open db");
    conn.execute(
        r#"
        INSERT INTO audit_records(session_id, record_id, kind, record_json)
        VALUES (?1, ?2, ?3, ?4)
        "#,
        rusqlite::params![
            "sess-legacy",
            "cand-legacy",
            "candidate",
            legacy.to_string()
        ],
    )
    .expect("insert legacy row");

    let loaded = store
        .load_record("sess-legacy", "cand-legacy")
        .expect("load result")
        .expect("record exists");
    assert!(
        loaded.ir_node_ids.is_empty(),
        "legacy rows should load with empty provenance vec"
    );
}

#[test]
fn append_llm_interaction_event_persists_event_with_payload() {
    let dir = tempfile::tempdir().expect("temp dir");
    let store = SessionStore::open(dir.path().join("sessions.sqlite")).expect("open store");
    store
        .create_session(&AuditSession::sample("sess-llm"))
        .expect("create session");

    store
        .append_llm_interaction_event(
            "sess-llm",
            &LlmInteractionEvent {
                provider: "openai".to_string(),
                model: Some("gpt-4.1-mini".to_string()),
                role: "SearchHints".to_string(),
                duration_ms: 22,
                prompt_chars: 80,
                response_chars: 120,
                attempt: 1,
                succeeded: true,
            },
        )
        .expect("append llm interaction event");

    let events = store.list_events("sess-llm").expect("list events");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, "llm.interaction");
    assert!(events[0].payload.contains("\"provider\":\"openai\""));
    assert!(events[0].payload.contains("\"succeeded\":true"));
}
