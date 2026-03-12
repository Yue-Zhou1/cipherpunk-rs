use audit_agent_core::session::AuditSession;
use session_store::SessionStore;

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
        .insert_searchable_record("sess-1", "nonce reuse in signer")
        .expect("insert searchable record");
    let hits = store.search_records("nonce").expect("search records");
    assert_eq!(hits.len(), 1);
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
