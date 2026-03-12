use anyhow::{bail, Result};
use rusqlite::Connection;

const SCHEMA_VERSION: i64 = 1;

pub fn initialize(conn: &Connection) -> Result<()> {
    let current: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if current > SCHEMA_VERSION {
        bail!(
            "database schema version {} is newer than supported {}",
            current,
            SCHEMA_VERSION
        );
    }

    if current == SCHEMA_VERSION {
        return Ok(());
    }

    if current != 0 {
        bail!(
            "unsupported schema migration path from {} to {}",
            current,
            SCHEMA_VERSION
        );
    }

    conn.execute_batch(
        r#"
        PRAGMA foreign_keys = ON;

        CREATE TABLE IF NOT EXISTS project_snapshots (
            snapshot_id TEXT PRIMARY KEY,
            snapshot_json TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE IF NOT EXISTS audit_sessions (
            session_id TEXT PRIMARY KEY,
            snapshot_id TEXT NOT NULL,
            selected_domains_json TEXT NOT NULL,
            ui_state_json TEXT NOT NULL,
            session_json TEXT NOT NULL,
            artifacts_dir TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY(snapshot_id) REFERENCES project_snapshots(snapshot_id)
        );

        CREATE TABLE IF NOT EXISTS audit_records (
            session_id TEXT NOT NULL,
            record_id TEXT NOT NULL,
            kind TEXT NOT NULL,
            record_json TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY (session_id, record_id),
            FOREIGN KEY(session_id) REFERENCES audit_sessions(session_id)
        );

        CREATE TABLE IF NOT EXISTS tool_runs (
            run_id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            tool_name TEXT NOT NULL,
            status TEXT NOT NULL,
            result_json TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY(session_id) REFERENCES audit_sessions(session_id)
        );

        CREATE TABLE IF NOT EXISTS evidence_artifacts (
            artifact_id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            relative_path TEXT NOT NULL,
            metadata_json TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY(session_id) REFERENCES audit_sessions(session_id)
        );

        CREATE TABLE IF NOT EXISTS checklist_runs (
            checklist_run_id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            checklist_id TEXT NOT NULL,
            status TEXT NOT NULL,
            result_json TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY(session_id) REFERENCES audit_sessions(session_id)
        );

        CREATE TABLE IF NOT EXISTS session_events (
            event_id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            event_json TEXT NOT NULL,
            created_at TEXT NOT NULL,
            FOREIGN KEY(session_id) REFERENCES audit_sessions(session_id)
        );

        CREATE VIRTUAL TABLE IF NOT EXISTS record_search USING fts5(
            session_id UNINDEXED,
            record_id UNINDEXED,
            content
        );

        PRAGMA user_version = 1;
        "#,
    )?;

    Ok(())
}
