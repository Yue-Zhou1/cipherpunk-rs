use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{Context, Result};
use audit_agent_core::session::{AuditRecord, AuditSession};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::schema;
use crate::search::RecordSearchHit;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionEvent {
    pub event_id: String,
    pub event_type: String,
    pub payload: String,
    pub created_at: DateTime<Utc>,
}

pub struct SessionStore {
    db_path: PathBuf,
    artifacts_root: PathBuf,
    conn: Mutex<Connection>,
}

impl SessionStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let db_path = path.as_ref().to_path_buf();
        if let Some(parent) = db_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create db parent dir {}", parent.display()))?;
        }

        let conn = Connection::open(&db_path)
            .with_context(|| format!("open sqlite db {}", db_path.display()))?;
        schema::initialize(&conn)?;

        let root_parent = db_path
            .parent()
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
        let artifacts_root = root_parent.join(".audit-sessions");
        fs::create_dir_all(&artifacts_root).with_context(|| {
            format!("create managed artifacts root {}", artifacts_root.display())
        })?;

        Ok(Self {
            db_path,
            artifacts_root,
            conn: Mutex::new(conn),
        })
    }

    pub fn create_session(&self, session: &AuditSession) -> Result<()> {
        let artifact_dir = self.artifacts_dir_for(&session.session_id);
        fs::create_dir_all(&artifact_dir)
            .with_context(|| format!("create session artifacts dir {}", artifact_dir.display()))?;

        let snapshot_json =
            serde_json::to_string(&session.snapshot).context("serialize project snapshot")?;
        let selected_domains_json =
            serde_json::to_string(&session.selected_domains).context("serialize domains")?;
        let ui_state_json =
            serde_json::to_string(&session.ui_state).context("serialize ui state")?;
        let session_json = serde_json::to_string(session).context("serialize session")?;
        let conn = self.conn.lock().expect("session-store mutex poisoned");

        conn.execute(
            r#"
            INSERT INTO project_snapshots(snapshot_id, snapshot_json)
            VALUES (?1, ?2)
            ON CONFLICT(snapshot_id) DO UPDATE SET
                snapshot_json = excluded.snapshot_json
            "#,
            params![session.snapshot.snapshot_id, snapshot_json],
        )?;

        conn.execute(
            r#"
            INSERT INTO audit_sessions(
                session_id,
                snapshot_id,
                selected_domains_json,
                ui_state_json,
                session_json,
                artifacts_dir,
                created_at,
                updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ON CONFLICT(session_id) DO UPDATE SET
                snapshot_id = excluded.snapshot_id,
                selected_domains_json = excluded.selected_domains_json,
                ui_state_json = excluded.ui_state_json,
                session_json = excluded.session_json,
                artifacts_dir = excluded.artifacts_dir,
                updated_at = excluded.updated_at
            "#,
            params![
                session.session_id,
                session.snapshot.snapshot_id,
                selected_domains_json,
                ui_state_json,
                session_json,
                artifact_dir.to_string_lossy().to_string(),
                session.created_at.to_rfc3339(),
                session.updated_at.to_rfc3339()
            ],
        )?;

        Ok(())
    }

    pub fn load_session(&self, session_id: &str) -> Result<Option<AuditSession>> {
        let conn = self.conn.lock().expect("session-store mutex poisoned");
        let json: Option<String> = conn
            .query_row(
                "SELECT session_json FROM audit_sessions WHERE session_id = ?1",
                params![session_id],
                |row| row.get(0),
            )
            .optional()?;
        json.map(|value| serde_json::from_str(&value).context("deserialize session"))
            .transpose()
    }

    pub fn upsert_record(&self, session_id: &str, record: &AuditRecord) -> Result<()> {
        let record_json = serde_json::to_string(record).context("serialize audit record")?;
        let conn = self.conn.lock().expect("session-store mutex poisoned");
        conn.execute(
            r#"
            INSERT INTO audit_records(session_id, record_id, kind, record_json)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(session_id, record_id) DO UPDATE SET
                kind = excluded.kind,
                record_json = excluded.record_json,
                updated_at = CURRENT_TIMESTAMP
            "#,
            params![session_id, record.record_id, kind_name(record), record_json],
        )?;

        conn.execute(
            "DELETE FROM record_search WHERE session_id = ?1 AND record_id = ?2",
            params![session_id, record.record_id],
        )?;
        conn.execute(
            "INSERT INTO record_search(session_id, record_id, content) VALUES (?1, ?2, ?3)",
            params![
                session_id,
                record.record_id,
                format!("{} {}", record.title, record.summary)
            ],
        )?;
        Ok(())
    }

    pub fn append_event(&self, session_id: &str, event: &SessionEvent) -> Result<()> {
        let event_json = serde_json::to_string(event).context("serialize session event")?;
        let conn = self.conn.lock().expect("session-store mutex poisoned");
        conn.execute(
            r#"
            INSERT INTO session_events(event_id, session_id, event_json, created_at)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(event_id) DO UPDATE SET
                session_id = excluded.session_id,
                event_json = excluded.event_json,
                created_at = excluded.created_at
            "#,
            params![
                event.event_id,
                session_id,
                event_json,
                event.created_at.to_rfc3339()
            ],
        )?;
        Ok(())
    }

    pub fn search_records(&self, query: &str) -> Result<Vec<RecordSearchHit>> {
        let conn = self.conn.lock().expect("session-store mutex poisoned");
        let mut stmt = conn.prepare(
            r#"
            SELECT session_id, record_id, content
            FROM record_search
            WHERE record_search MATCH ?1
            ORDER BY rank
            "#,
        )?;

        let rows = stmt.query_map(params![query], |row| {
            Ok(RecordSearchHit {
                session_id: row.get(0)?,
                record_id: row.get(1)?,
                snippet: row.get(2)?,
            })
        })?;

        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn insert_searchable_record(&self, session_id: &str, content: &str) -> Result<()> {
        let record_id = format!("manual-{}", Utc::now().timestamp_nanos_opt().unwrap_or(0));
        let conn = self.conn.lock().expect("session-store mutex poisoned");
        conn.execute(
            "INSERT INTO record_search(session_id, record_id, content) VALUES (?1, ?2, ?3)",
            params![session_id, record_id, content],
        )?;
        Ok(())
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    fn artifacts_dir_for(&self, session_id: &str) -> PathBuf {
        self.artifacts_root.join(session_id).join("artifacts")
    }
}

fn kind_name(record: &AuditRecord) -> &'static str {
    match record.kind {
        audit_agent_core::session::AuditRecordKind::ReviewNote => "review_note",
        audit_agent_core::session::AuditRecordKind::Candidate => "candidate",
        audit_agent_core::session::AuditRecordKind::Finding => "finding",
    }
}
