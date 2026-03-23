use std::path::PathBuf;

use chrono::Utc;

use crate::jobs::{AuditJob, AuditJobStatus};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditEvent {
    EngineCompleted {
        engine: String,
        findings_count: usize,
        duration_ms: u64,
    },
    EngineFailed {
        engine: String,
        reason: String,
    },
    AuditCompleted {
        audit_id: String,
        output_dir: PathBuf,
        finding_count: usize,
    },
}

pub trait AuditEventSink: Send + Sync {
    fn emit(&self, event: AuditEvent);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobLifecycleEvent {
    pub job_id: String,
    pub status: AuditJobStatus,
    pub payload: String,
}

impl JobLifecycleEvent {
    pub fn queued(job: &AuditJob) -> anyhow::Result<Self> {
        let payload = serde_json::to_string(job)?;
        Ok(Self {
            job_id: format!("job:{}:queued", job.job_id),
            status: AuditJobStatus::Queued,
            payload,
        })
    }

    pub fn to_session_event(&self) -> session_store::SessionEvent {
        session_store::SessionEvent {
            event_id: self.job_id.clone(),
            event_type: "job.lifecycle".to_string(),
            payload: self.payload.clone(),
            created_at: Utc::now(),
        }
    }
}
