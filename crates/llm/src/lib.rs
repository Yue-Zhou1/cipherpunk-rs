pub mod evidence_gate;
pub mod provider;

pub use evidence_gate::{EvidenceGate, GateResult, HarnessCode};
pub use provider::{
    AnthropicProvider, CompletionOpts, LlmProvider, LlmRole, OllamaProvider, OpenAiProvider,
    TemplateFallback, llm_call, provider_from_env,
};
