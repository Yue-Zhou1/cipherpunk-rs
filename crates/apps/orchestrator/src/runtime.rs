use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use audit_agent_core::LlmProvider;
use audit_agent_core::audit_config::{
    AuditConfig, BudgetConfig, EngineConfig, LlmConfig, OptionalInputs, ResolvedScope,
    ResolvedSource, SourceOrigin,
};
use audit_agent_core::engine::{
    EvidenceWriter, NoopEvidenceWriter, NoopSandboxRunner, SandboxRunner,
};
use audit_agent_core::llm::{LlmRequest, LlmResponse, LlmRole};
use audit_agent_core::workspace::CargoWorkspace;

#[derive(Clone)]
pub struct OrchestratorRuntime {
    pub sandbox: Arc<dyn SandboxRunner>,
    pub evidence_writer: Arc<dyn EvidenceWriter>,
    pub context_llm: Option<Arc<dyn LlmProvider>>,
}

impl Default for OrchestratorRuntime {
    fn default() -> Self {
        Self {
            sandbox: Arc::new(NoopSandboxRunner),
            evidence_writer: Arc::new(NoopEvidenceWriter),
            context_llm: None,
        }
    }
}

impl OrchestratorRuntime {
    pub fn for_tests() -> Self {
        Self {
            context_llm: Some(Arc::new(TestContextLlmProvider)),
            ..Self::default()
        }
    }
}

pub fn build_test_config() -> AuditConfig {
    AuditConfig {
        audit_id: "audit-orchestrator-runtime-test".to_string(),
        source: ResolvedSource {
            local_path: PathBuf::new(),
            origin: SourceOrigin::Local {
                original_path: PathBuf::new(),
            },
            commit_hash: String::new(),
            content_hash: String::new(),
        },
        scope: ResolvedScope {
            target_crates: vec![],
            excluded_crates: vec![],
            build_matrix: vec![],
            detected_frameworks: vec![],
        },
        engines: EngineConfig {
            crypto_zk: true,
            distributed: false,
        },
        budget: BudgetConfig {
            kani_timeout_secs: 60,
            z3_timeout_secs: 60,
            fuzz_duration_secs: 60,
            madsim_ticks: 10,
            max_llm_retries: 1,
            semantic_index_timeout_secs: 30,
        },
        optional_inputs: OptionalInputs {
            spec_document: None,
            previous_audit: None,
            custom_invariants: vec![],
            known_entry_points: vec![],
        },
        llm: LlmConfig {
            api_key_present: false,
            provider: None,
            no_llm_prose: false,
        },
        output_dir: PathBuf::new(),
    }
}

pub fn build_test_workspace() -> CargoWorkspace {
    CargoWorkspace::default()
}

#[derive(Debug, Clone, Default)]
pub struct TestContextLlmProvider;

#[async_trait]
impl LlmProvider for TestContextLlmProvider {
    fn provider_name(&self) -> &str {
        "orchestrator-test-llm"
    }

    async fn complete(&self, request: &LlmRequest) -> Result<LlmResponse> {
        Ok(LlmResponse {
            text: format!("[{}] {}", role_label(&request.role), request.prompt),
            model: "test-model".to_string(),
        })
    }
}

fn role_label(role: &LlmRole) -> &'static str {
    match role {
        LlmRole::Scaffolding => "scaffolding",
        LlmRole::SearchHints => "search",
        LlmRole::ProseRendering => "prose",
        LlmRole::LeanScaffold => "lean",
    }
}
