use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Result, anyhow, bail};
use async_trait::async_trait;
use audit_agent_core::finding::VerificationStatus;
use audit_agent_core::session::{AuditRecord, AuditRecordKind};
use serde::de::DeserializeOwned;

use crate::enforcement::{
    ContractEnforcer, EnforcedResponse, LlmInteractionHook, retry_policy_for_role,
};
use crate::provider::{CompletionOpts, LlmProvider, LlmRole};
use crate::sanitize::{GraphContextEntry, pack_graph_aware_context, sanitize_prompt_input};
use crate::semantic_memory::format_semantic_signatures;

pub use crate::contracts::DomainPlan;
pub use crate::contracts::{ArchitectureOverview, CandidateDraft, ChecklistPlan};
pub use crate::semantic_memory::SemanticSignatureContext;

static RECORD_COUNTER: AtomicU64 = AtomicU64::new(1);
const DEFAULT_PROMPT_CONTEXT_BUDGET: usize = 2_000;

pub struct CopilotService {
    provider: Arc<dyn LlmProvider>,
    interaction_hook: Option<LlmInteractionHook>,
}

impl CopilotService {
    pub fn new(provider: Arc<dyn LlmProvider>) -> Self {
        Self {
            provider,
            interaction_hook: None,
        }
    }

    pub fn with_mock_json(mock_json: &str) -> Self {
        Self::new(Arc::new(MockJsonProvider {
            json: mock_json.to_string(),
        }))
    }

    pub fn with_interaction_hook(mut self, hook: LlmInteractionHook) -> Self {
        self.interaction_hook = Some(hook);
        self
    }

    pub async fn plan_checklists(&self, workspace_summary: &str) -> Result<ChecklistPlan> {
        let task_description = format!(
            "Select applicable audit domains for this workspace:\n{}",
            sanitize_prompt_input(workspace_summary)
        );
        let plan = self
            .enforce_contract::<ChecklistPlan>(
                LlmRole::SearchHints,
                "ChecklistPlan",
                &task_description,
            )
            .await?
            .value;
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
        let task_description = format!(
            "Generate architecture overview fields for:\n{}",
            sanitize_prompt_input(workspace_summary)
        );
        let overview = self
            .enforce_contract::<ArchitectureOverview>(
                LlmRole::SearchHints,
                "ArchitectureOverview",
                &task_description,
            )
            .await?
            .value;
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
        self.generate_candidate_with_context(hotspot, "", &[]).await
    }

    pub async fn generate_candidate_with_context(
        &self,
        hotspot: &str,
        source_context: &str,
        graph_context: &[GraphContextEntry],
    ) -> Result<AuditRecord> {
        self.generate_candidate_with_semantic_context(hotspot, source_context, graph_context, &[])
            .await
    }

    pub async fn generate_candidate_with_semantic_context(
        &self,
        hotspot: &str,
        source_context: &str,
        graph_context: &[GraphContextEntry],
        semantic_signatures: &[SemanticSignatureContext],
    ) -> Result<AuditRecord> {
        let packed_context =
            pack_graph_aware_context(source_context, graph_context, DEFAULT_PROMPT_CONTEXT_BUDGET);
        let task_description = format!(
            "Generate a concise candidate for hotspot:\n{}\n\nContext:\n{}\n\nHistorical signatures:\n{}",
            sanitize_prompt_input(hotspot),
            packed_context,
            format_semantic_signatures(semantic_signatures)
        );
        let policy = retry_policy_for_role(&LlmRole::SearchHints);
        let enforcer =
            ContractEnforcer::<CandidateDraft>::new(LlmRole::SearchHints, "CandidateDraft")
                .with_retry(policy);
        let draft = enforcer
            .execute(
                &*self.provider,
                &task_description,
                &CompletionOpts::default(),
                self.interaction_hook.as_ref(),
            )
            .await?
            .value;
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

    async fn enforce_contract<T>(
        &self,
        role: LlmRole,
        contract_name: &str,
        task_description: &str,
    ) -> Result<EnforcedResponse<T>>
    where
        T: DeserializeOwned + Clone + Default,
    {
        let policy = retry_policy_for_role(&role);
        let enforcer = ContractEnforcer::<T>::new(role, contract_name)
            .with_retry(policy)
            .with_fallback(T::default());
        enforcer
            .execute(
                &*self.provider,
                task_description,
                &CompletionOpts::default(),
                self.interaction_hook.as_ref(),
            )
            .await
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
