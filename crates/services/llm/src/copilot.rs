use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Result, anyhow, bail};
use async_trait::async_trait;
use audit_agent_core::finding::VerificationStatus;
use audit_agent_core::session::{AuditRecord, AuditRecordKind};

use crate::provider::{CompletionOpts, LlmProvider, LlmRole, json_only_prompt, llm_call};
use crate::sanitize::{parse_json_contract, sanitize_prompt_input};

pub use crate::contracts::DomainPlan;
pub use crate::contracts::{ArchitectureOverview, CandidateDraft, ChecklistPlan};

static RECORD_COUNTER: AtomicU64 = AtomicU64::new(1);

pub struct CopilotService {
    provider: Arc<dyn LlmProvider>,
}

impl CopilotService {
    pub fn new(provider: Arc<dyn LlmProvider>) -> Self {
        Self { provider }
    }

    pub fn with_mock_json(mock_json: &str) -> Self {
        Self::new(Arc::new(MockJsonProvider {
            json: mock_json.to_string(),
        }))
    }

    pub async fn plan_checklists(&self, workspace_summary: &str) -> Result<ChecklistPlan> {
        let prompt = json_only_prompt(
            "ChecklistPlan",
            &format!(
                "Select applicable audit domains for this workspace:\n{}",
                sanitize_prompt_input(workspace_summary)
            ),
        );
        let plan: ChecklistPlan = self.complete_json(LlmRole::SearchHints, &prompt).await?;
        if plan
            .domains
            .iter()
            .any(|domain| domain.id.trim().is_empty())
        {
            bail!("checklist domain id must not be empty");
        }
        if plan
            .domains
            .iter()
            .any(|domain| domain.rationale.trim().is_empty())
        {
            bail!("checklist domain rationale must not be empty");
        }
        Ok(plan)
    }

    pub async fn generate_overview_note(&self, workspace_summary: &str) -> Result<AuditRecord> {
        let prompt = json_only_prompt(
            "ArchitectureOverview",
            &format!(
                "Generate architecture overview fields for:\n{}",
                sanitize_prompt_input(workspace_summary)
            ),
        );
        let overview: ArchitectureOverview =
            self.complete_json(LlmRole::SearchHints, &prompt).await?;
        let summary = format!(
            "assets={}, trust_boundaries={}, hotspots={}",
            overview.assets.len(),
            overview.trust_boundaries.len(),
            overview.hotspots.len()
        );
        Ok(AuditRecord {
            record_id: next_record_id("NOTE"),
            kind: AuditRecordKind::ReviewNote,
            title: "AI architecture overview".to_string(),
            summary,
            severity: None,
            verification_status: VerificationStatus::unverified(
                "AI-generated overview requires analyst review",
            ),
            locations: vec![],
            evidence_refs: vec![],
            labels: vec!["ai-generated".to_string(), "overview".to_string()],
            ir_node_ids: vec![],
        })
    }

    pub async fn generate_candidate(&self, hotspot: &str) -> Result<AuditRecord> {
        let prompt = json_only_prompt(
            "CandidateDraft",
            &format!(
                "Generate a concise candidate for hotspot:\n{}",
                sanitize_prompt_input(hotspot)
            ),
        );
        let draft: CandidateDraft = self.complete_json(LlmRole::SearchHints, &prompt).await?;
        if draft.title.trim().is_empty() {
            return Err(anyhow!("candidate title must not be empty"));
        }
        if draft.summary.trim().is_empty() {
            return Err(anyhow!("candidate summary must not be empty"));
        }

        // Trust boundary: AI may only produce unverified material.
        Ok(AuditRecord {
            record_id: next_record_id("CAND"),
            kind: AuditRecordKind::Candidate,
            title: draft.title,
            summary: draft.summary,
            severity: None,
            verification_status: VerificationStatus::unverified(
                "AI-generated candidate requires deterministic validation",
            ),
            locations: vec![],
            evidence_refs: vec![],
            labels: vec!["ai-generated".to_string(), "candidate".to_string()],
            ir_node_ids: vec![],
        })
    }

    async fn complete_json<T>(&self, role: LlmRole, prompt: &str) -> Result<T>
    where
        T: serde::de::DeserializeOwned,
    {
        let response = llm_call(&*self.provider, role, prompt, &CompletionOpts::default()).await?;
        parse_json_contract(&response)
    }
}

#[derive(Debug, Clone)]
struct MockJsonProvider {
    json: String,
}

#[async_trait]
impl LlmProvider for MockJsonProvider {
    async fn complete(&self, _prompt: &str, _opts: &CompletionOpts) -> Result<String> {
        Ok(self.json.clone())
    }

    fn name(&self) -> &str {
        "mock-json"
    }

    fn is_available(&self) -> bool {
        true
    }
}

fn next_record_id(prefix: &str) -> String {
    let counter = RECORD_COUNTER.fetch_add(1, Ordering::Relaxed);
    let timestamp_nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id();
    format!("{prefix}-{pid}-{timestamp_nanos}-{counter}")
}
