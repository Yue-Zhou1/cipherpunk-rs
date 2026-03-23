use std::path::PathBuf;

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::jobs::{AuditJob, AuditJobStatus};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
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
    LlmInteraction {
        role: String,
        provider: String,
        model: Option<String>,
        prompt_chars: usize,
        response_chars: usize,
        duration_ms: u64,
        succeeded: bool,
    },
    ToolActionCompleted {
        action_id: String,
        tool_family: String,
        target: String,
        status: String,
        duration_ms: u64,
    },
    ReviewDecisionApplied {
        record_id: String,
        action: String,
        analyst_note: Option<String>,
    },
    ProviderFailover {
        from: String,
        to: String,
        role: String,
        reason: String,
    },
    AdviserConsulted {
        engine: String,
        suggestion: String,
        applied: bool,
    },
}

impl AuditEvent {
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::EngineCompleted { .. } => "engine.completed",
            Self::EngineFailed { .. } => "engine.failed",
            Self::AuditCompleted { .. } => "audit.completed",
            Self::LlmInteraction { .. } => "llm.interaction",
            Self::ToolActionCompleted { .. } => "tool.action.completed",
            Self::ReviewDecisionApplied { .. } => "review.decision",
            Self::ProviderFailover { .. } => "provider.failover",
            Self::AdviserConsulted { .. } => "adviser.consulted",
        }
    }

    pub fn to_session_event(&self, session_id: &str) -> session_store::SessionEvent {
        let now = Utc::now();
        session_store::SessionEvent {
            event_id: format!(
                "{}:{}:{}",
                self.event_type(),
                session_id,
                now.timestamp_micros()
            ),
            event_type: self.event_type().to_string(),
            payload: serde_json::to_string(self).unwrap_or_default(),
            created_at: now,
        }
    }
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

#[cfg(test)]
mod tests {
    use super::AuditEvent;

    #[test]
    fn audit_event_type_maps_new_observability_variants() {
        let cases = vec![
            (
                AuditEvent::LlmInteraction {
                    role: "SearchHints".to_string(),
                    provider: "openai".to_string(),
                    model: Some("gpt-4.1-mini".to_string()),
                    prompt_chars: 128,
                    response_chars: 256,
                    duration_ms: 42,
                    succeeded: true,
                },
                "llm.interaction",
            ),
            (
                AuditEvent::ToolActionCompleted {
                    action_id: "action-1".to_string(),
                    tool_family: "kani".to_string(),
                    target: "crate-a".to_string(),
                    status: "Completed".to_string(),
                    duration_ms: 15,
                },
                "tool.action.completed",
            ),
            (
                AuditEvent::ReviewDecisionApplied {
                    record_id: "cand-1".to_string(),
                    action: "confirm".to_string(),
                    analyst_note: Some("validated against trace".to_string()),
                },
                "review.decision",
            ),
            (
                AuditEvent::ProviderFailover {
                    from: "openai".to_string(),
                    to: "template-fallback".to_string(),
                    role: "Scaffolding".to_string(),
                    reason: "transient failure".to_string(),
                },
                "provider.failover",
            ),
            (
                AuditEvent::AdviserConsulted {
                    engine: "z3-engine".to_string(),
                    suggestion: "RetryWithRelaxedBudget".to_string(),
                    applied: true,
                },
                "adviser.consulted",
            ),
        ];

        for (event, expected_type) in cases {
            assert_eq!(event.event_type(), expected_type);
        }
    }

    #[test]
    fn audit_event_to_session_event_uses_event_type_and_payload() {
        let event = AuditEvent::ToolActionCompleted {
            action_id: "action-2".to_string(),
            tool_family: "fuzz".to_string(),
            target: "crate-b".to_string(),
            status: "Failed".to_string(),
            duration_ms: 99,
        };

        let session_event = event.to_session_event("sess-1");
        assert_eq!(session_event.event_type, "tool.action.completed");
        assert!(
            session_event.event_id.starts_with("tool.action.completed:"),
            "event_id should include event type prefix"
        );
        assert!(session_event.payload.contains("\"action_id\":\"action-2\""));
        assert!(session_event.payload.contains("\"status\":\"Failed\""));
    }
}
