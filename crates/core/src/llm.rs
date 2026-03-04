use anyhow::Result;
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum LlmRole {
    MechanicalScaffolding,
    SearchSpaceGuidance,
    ProseRendering,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CompletionOpts {
    pub temperature_millis: u16,
    pub max_tokens: u32,
}

impl Default for CompletionOpts {
    fn default() -> Self {
        Self {
            temperature_millis: 200,
            max_tokens: 1024,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct LlmRequest {
    pub role: LlmRole,
    pub prompt: String,
    pub opts: CompletionOpts,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct LlmResponse {
    pub text: String,
    pub model: String,
}

#[async_trait]
pub trait LlmProvider: Send + Sync {
    fn provider_name(&self) -> &str;
    async fn complete(&self, request: &LlmRequest) -> Result<LlmResponse>;
}
