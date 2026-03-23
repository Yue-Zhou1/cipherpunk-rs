pub mod contracts;
pub mod copilot;
pub mod enforcement;
pub mod evidence_gate;
pub mod provider;
pub mod sanitize;
pub mod semantic_memory;

pub use contracts::{ArchitectureOverview, CandidateDraft, ChecklistPlan, DomainPlan};
pub use copilot::CopilotService;
pub use enforcement::{
    ContractEnforcer, EnforcedResponse, LlmInteractionHook, RetryPolicy, retry_policy_for_role,
};
pub use evidence_gate::{EvidenceGate, GateResult, HarnessCode};
#[allow(deprecated)]
pub use provider::{
    AnthropicProvider, CompletionOpts, LlmProvenance, LlmProvider, LlmRole, OllamaProvider,
    OpenAiProvider, TemplateFallback, json_only_prompt, llm_call, llm_call_traced,
    provider_from_env,
};
