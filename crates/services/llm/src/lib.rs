pub mod adviser;
pub mod contracts;
pub mod copilot;
pub mod enforcement;
pub mod evidence_gate;
pub mod provider;
pub mod role_config;
pub mod sanitize;
pub mod semantic_memory;

pub use adviser::{
    AdviserAction, AdviserBudgetSnapshot, AdviserContext, AdviserService, AdviserSuggestion,
};
pub use contracts::{ArchitectureOverview, CandidateDraft, ChecklistPlan, DomainPlan};
pub use copilot::CopilotService;
pub use enforcement::{
    ContractEnforcer, EnforcedResponse, LlmInteractionHook, RetryPolicy, retry_policy_for_role,
};
pub use evidence_gate::{EvidenceGate, GateResult, HarnessCode};
#[allow(deprecated)]
pub use provider::{
    AnthropicProvider, CompletionOpts, LlmProvenance, LlmProvider, LlmRole, OllamaProvider,
    OpenAiProvider, TemplateFallback, is_transient_error, json_only_prompt, llm_call,
    llm_call_traced, provider_from_env, provider_from_name,
};
pub use role_config::{
    LlmRoleConfigMap, ProviderFailoverHook, ProviderFailoverRecord, RoleAwareProvider, RoleConfig,
    role_aware_llm_call, role_aware_provider_from_env,
};
