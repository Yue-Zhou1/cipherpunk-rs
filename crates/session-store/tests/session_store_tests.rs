use audit_agent_core::finding::VerificationStatus;
use audit_agent_core::session::{AuditRecord, AuditSession};
use chrono::Utc;
use session_store::{SessionEvent, SessionStore};

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
