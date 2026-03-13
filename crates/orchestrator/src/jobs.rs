use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditJobKind {
    BuildProjectIr,
    GenerateAiOverview,
    PlanChecklists,
    RunDomainChecklist { domain_id: String },
    RunToolAction { action_id: String },
    ExportReports,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditJobStatus {
    Queued,
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditJob {
    pub job_id: String,
    pub session_id: String,
    pub kind: AuditJobKind,
    pub status: AuditJobStatus,
    pub created_at: DateTime<Utc>,
}

impl AuditJob {
    pub fn queued(session_id: &str, kind: AuditJobKind, sequence: usize) -> Self {
        let now = Utc::now();
        Self {
            job_id: format!("job-{}-{sequence}", now.timestamp_micros()),
            session_id: session_id.to_string(),
            kind,
            status: AuditJobStatus::Queued,
            created_at: now,
        }
    }
}
