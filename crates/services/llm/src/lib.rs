pub mod contracts;
pub mod copilot;
pub mod evidence_gate;
pub mod provider;
pub mod sanitize;
pub mod semantic_memory;

pub use contracts::{ArchitectureOverview, CandidateDraft, ChecklistPlan, DomainPlan};
pub use copilot::CopilotService;
pub use evidence_gate::{EvidenceGate, GateResult, HarnessCode};
pub use provider::{
    AnthropicProvider, CompletionOpts, LlmProvider, LlmRole, OllamaProvider, OpenAiProvider,
    TemplateFallback, json_only_prompt, llm_call, provider_from_env,
};
