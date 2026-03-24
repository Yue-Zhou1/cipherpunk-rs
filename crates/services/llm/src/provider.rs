use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
pub use audit_agent_core::llm::LlmRole;
use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionOpts {
    pub temperature_millis: u16,
    pub max_tokens: u32,
}

impl Default for CompletionOpts {
    fn default() -> Self {
        Self {
            temperature_millis: 100,
            max_tokens: 1024,
        }
    }
}

#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn complete(&self, prompt: &str, opts: &CompletionOpts) -> Result<String>;

    async fn complete_with_role(
        &self,
        _role: &LlmRole,
        prompt: &str,
        opts: &CompletionOpts,
    ) -> Result<LlmCallOutput> {
        let response = self.complete(prompt, opts).await?;
        Ok(LlmCallOutput {
            response,
            provider: self.name().to_string(),
            model: self.model().map(|value| value.to_string()),
        })
    }

    async fn complete_with_role_and_model(
        &self,
        role: &LlmRole,
        prompt: &str,
        opts: &CompletionOpts,
        model_override: Option<&str>,
    ) -> Result<LlmCallOutput> {
        let _ = model_override;
        self.complete_with_role(role, prompt, opts).await
    }

    fn name(&self) -> &str;
    fn is_available(&self) -> bool;
    fn model(&self) -> Option<&str> {
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LlmProvenance {
    pub provider: String,
    pub model: Option<String>,
    pub role: String,
    pub duration_ms: u64,
    pub prompt_chars: usize,
    pub response_chars: usize,
    pub attempt: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmCallOutput {
    pub response: String,
    pub provider: String,
    pub model: Option<String>,
}

#[derive(Debug, Clone)]
pub struct OpenAiProvider {
    pub api_key: String,
    pub model: String,
    pub base_url: String,
    client: Client,
}

#[derive(Debug, Clone)]
pub struct AnthropicProvider {
    pub api_key: String,
    pub model: String,
    pub base_url: String,
    client: Client,
}

#[derive(Debug, Clone)]
pub struct OllamaProvider {
    pub base_url: String,
    pub model: String,
    client: Client,
}

#[derive(Debug, Clone, Copy)]
pub struct TemplateFallback;

#[async_trait]
impl LlmProvider for OpenAiProvider {
    async fn complete(&self, prompt: &str, opts: &CompletionOpts) -> Result<String> {
        self.complete_with_model(prompt, opts, &self.model).await
    }

    async fn complete_with_role_and_model(
        &self,
        _role: &LlmRole,
        prompt: &str,
        opts: &CompletionOpts,
        model_override: Option<&str>,
    ) -> Result<LlmCallOutput> {
        let model = model_override.unwrap_or(&self.model);
        let response = self.complete_with_model(prompt, opts, model).await?;
        Ok(LlmCallOutput {
            response,
            provider: self.name().to_string(),
            model: Some(model.to_string()),
        })
    }

    fn name(&self) -> &str {
        "openai"
    }

    fn is_available(&self) -> bool {
        !self.api_key.trim().is_empty()
    }

    fn model(&self) -> Option<&str> {
        Some(&self.model)
    }
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    async fn complete(&self, prompt: &str, opts: &CompletionOpts) -> Result<String> {
        self.complete_with_model(prompt, opts, &self.model).await
    }

    async fn complete_with_role_and_model(
        &self,
        _role: &LlmRole,
        prompt: &str,
        opts: &CompletionOpts,
        model_override: Option<&str>,
    ) -> Result<LlmCallOutput> {
        let model = model_override.unwrap_or(&self.model);
        let response = self.complete_with_model(prompt, opts, model).await?;
        Ok(LlmCallOutput {
            response,
            provider: self.name().to_string(),
            model: Some(model.to_string()),
        })
    }

    fn name(&self) -> &str {
        "anthropic"
    }

    fn is_available(&self) -> bool {
        !self.api_key.trim().is_empty()
    }

    fn model(&self) -> Option<&str> {
        Some(&self.model)
    }
}

#[async_trait]
impl LlmProvider for OllamaProvider {
    async fn complete(&self, prompt: &str, opts: &CompletionOpts) -> Result<String> {
        self.complete_with_model(prompt, opts, &self.model).await
    }

    async fn complete_with_role_and_model(
        &self,
        _role: &LlmRole,
        prompt: &str,
        opts: &CompletionOpts,
        model_override: Option<&str>,
    ) -> Result<LlmCallOutput> {
        let model = model_override.unwrap_or(&self.model);
        let response = self.complete_with_model(prompt, opts, model).await?;
        Ok(LlmCallOutput {
            response,
            provider: self.name().to_string(),
            model: Some(model.to_string()),
        })
    }

    fn name(&self) -> &str {
        "ollama"
    }

    fn is_available(&self) -> bool {
        !self.base_url.trim().is_empty()
    }

    fn model(&self) -> Option<&str> {
        Some(&self.model)
    }
}

#[async_trait]
impl LlmProvider for TemplateFallback {
    async fn complete(&self, prompt: &str, _opts: &CompletionOpts) -> Result<String> {
        Ok(template_library::match_prompt(prompt))
    }

    fn name(&self) -> &str {
        "template-fallback"
    }

    fn is_available(&self) -> bool {
        true
    }
}

pub fn provider_from_env() -> Box<dyn LlmProvider> {
    let requested = std::env::var("LLM_PROVIDER")
        .ok()
        .map(|provider| provider.trim().to_ascii_lowercase());

    match requested.as_deref() {
        Some(provider_name) => provider_from_name(provider_name),
        None => {
            if let Some(provider) = openai_provider() {
                Box::new(provider)
            } else if let Some(provider) = anthropic_provider() {
                Box::new(provider)
            } else if let Some(provider) = ollama_provider() {
                Box::new(provider)
            } else {
                Box::new(TemplateFallback)
            }
        }
    }
}

pub fn provider_from_name(provider_name: &str) -> Box<dyn LlmProvider> {
    match provider_name.trim().to_ascii_lowercase().as_str() {
        "openai" => openai_provider()
            .map(|provider| Box::new(provider) as Box<dyn LlmProvider>)
            .unwrap_or_else(|| Box::new(TemplateFallback)),
        "anthropic" => anthropic_provider()
            .map(|provider| Box::new(provider) as Box<dyn LlmProvider>)
            .unwrap_or_else(|| Box::new(TemplateFallback)),
        "ollama" => ollama_provider()
            .map(|provider| Box::new(provider) as Box<dyn LlmProvider>)
            .unwrap_or_else(|| Box::new(TemplateFallback)),
        "template" | "template-fallback" => Box::new(TemplateFallback),
        _ => Box::new(TemplateFallback),
    }
}

/// Classify an LLM call error as transient (retryable) or permanent.
pub fn is_transient_error(err: &anyhow::Error) -> bool {
    let msg = err.to_string().to_ascii_lowercase();

    // HTTP status classes commonly associated with provider-side instability.
    if msg.contains("429") || msg.contains("rate limit") {
        return true;
    }
    if msg.contains("500") || msg.contains("internal server error") {
        return true;
    }
    if msg.contains("502") || msg.contains("bad gateway") {
        return true;
    }
    if msg.contains("503") || msg.contains("service unavailable") {
        return true;
    }
    if msg.contains("504") || msg.contains("gateway timeout") {
        return true;
    }

    // Network/transient transport failures.
    if msg.contains("timed out")
        || msg.contains("timeout")
        || msg.contains("connection refused")
        || msg.contains("connection reset")
        || msg.contains("dns")
    {
        return true;
    }

    false
}

pub(crate) fn openai_provider() -> Option<OpenAiProvider> {
    let api_key = std::env::var("LLM_API_KEY").ok()?;
    if api_key.trim().is_empty() {
        return None;
    }
    OpenAiProvider::new(
        api_key,
        std::env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4o-mini".to_string()),
        std::env::var("OPENAI_BASE_URL").unwrap_or_else(|_| "https://api.openai.com".to_string()),
    )
    .ok()
}

pub(crate) fn anthropic_provider() -> Option<AnthropicProvider> {
    let api_key = std::env::var("ANTHROPIC_API_KEY").ok()?;
    if api_key.trim().is_empty() {
        return None;
    }
    AnthropicProvider::new(
        api_key,
        std::env::var("ANTHROPIC_MODEL").unwrap_or_else(|_| "claude-3-5-sonnet".to_string()),
        std::env::var("ANTHROPIC_BASE_URL")
            .unwrap_or_else(|_| "https://api.anthropic.com".to_string()),
    )
    .ok()
}

pub(crate) fn ollama_provider() -> Option<OllamaProvider> {
    let base_url = std::env::var("OLLAMA_BASE_URL").ok()?;
    if base_url.trim().is_empty() {
        return None;
    }
    OllamaProvider::new(
        base_url,
        std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| "llama3".to_string()),
    )
    .ok()
}

#[deprecated(note = "Use llm_call_traced to capture response provenance.")]
pub async fn llm_call(
    provider: &dyn LlmProvider,
    role: LlmRole,
    prompt: &str,
    opts: &CompletionOpts,
) -> Result<String> {
    let (response, _provenance) = llm_call_traced(provider, role, prompt, opts).await?;
    Ok(response)
}

pub async fn llm_call_traced(
    provider: &dyn LlmProvider,
    role: LlmRole,
    prompt: &str,
    opts: &CompletionOpts,
) -> Result<(String, LlmProvenance)> {
    let started_at = std::time::Instant::now();
    tracing::debug!(role = ?role, provider = provider.name(), "LLM call");
    let output = provider.complete_with_role(&role, prompt, opts).await?;
    let response = output.response;
    let duration_ms = started_at.elapsed().as_millis() as u64;
    tracing::info!(
        role = ?role,
        provider = output.provider.as_str(),
        model = output.model.as_deref().unwrap_or("unknown"),
        duration_ms,
        response_chars = response.len(),
        "LLM call completed"
    );
    let provenance = LlmProvenance {
        provider: output.provider,
        model: output.model,
        role: format!("{role:?}"),
        duration_ms,
        prompt_chars: prompt.len(),
        response_chars: response.len(),
        attempt: 1,
    };
    Ok((response, provenance))
}

pub fn json_only_prompt(contract_name: &str, task: &str) -> String {
    format!(
        "Return only valid JSON for contract `{contract_name}`.\n\
         Do not include markdown, prose, or code fences.\n\
         Task:\n{task}"
    )
}

fn http_client() -> Result<Client> {
    Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(120))
        .build()
        .context("failed to build HTTP client for LLM provider")
}

impl OpenAiProvider {
    pub fn new(api_key: String, model: String, base_url: String) -> Result<Self> {
        Ok(Self {
            api_key,
            model,
            base_url,
            client: http_client()?,
        })
    }

    async fn complete_with_model(
        &self,
        prompt: &str,
        opts: &CompletionOpts,
        model: &str,
    ) -> Result<String> {
        if !self.is_available() {
            return Err(anyhow!("OpenAI API key is missing"));
        }

        let url = format!(
            "{}/v1/chat/completions",
            trim_trailing_slash(&self.base_url)
        );
        let payload = serde_json::json!({
            "model": model,
            "messages": [{ "role": "user", "content": prompt }],
            "temperature": temperature(opts),
            "max_tokens": opts.max_tokens,
        });

        let response = self
            .client
            .post(url)
            .bearer_auth(&self.api_key)
            .json(&payload)
            .send()
            .await?;

        parse_openai_response(response).await
    }
}

impl AnthropicProvider {
    pub fn new(api_key: String, model: String, base_url: String) -> Result<Self> {
        Ok(Self {
            api_key,
            model,
            base_url,
            client: http_client()?,
        })
    }

    async fn complete_with_model(
        &self,
        prompt: &str,
        opts: &CompletionOpts,
        model: &str,
    ) -> Result<String> {
        if !self.is_available() {
            return Err(anyhow!("Anthropic API key is missing"));
        }

        let url = format!("{}/v1/messages", trim_trailing_slash(&self.base_url));
        let payload = serde_json::json!({
            "model": model,
            "messages": [{ "role": "user", "content": prompt }],
            "temperature": temperature(opts),
            "max_tokens": opts.max_tokens,
        });

        let response = self
            .client
            .post(url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&payload)
            .send()
            .await?;

        parse_anthropic_response(response).await
    }
}

impl OllamaProvider {
    pub fn new(base_url: String, model: String) -> Result<Self> {
        Ok(Self {
            base_url,
            model,
            client: http_client()?,
        })
    }

    async fn complete_with_model(
        &self,
        prompt: &str,
        opts: &CompletionOpts,
        model: &str,
    ) -> Result<String> {
        if !self.is_available() {
            return Err(anyhow!("Ollama base URL is missing"));
        }

        let url = format!("{}/api/generate", trim_trailing_slash(&self.base_url));
        let payload = serde_json::json!({
            "model": model,
            "prompt": prompt,
            "stream": false,
            "options": {
                "temperature": temperature(opts),
                "num_predict": opts.max_tokens,
            }
        });

        let response = self.client.post(url).json(&payload).send().await?;
        parse_ollama_response(response).await
    }
}

fn trim_trailing_slash(value: &str) -> &str {
    value.trim_end_matches('/')
}

fn temperature(opts: &CompletionOpts) -> f32 {
    f32::from(opts.temperature_millis) / 1_000.0
}

#[derive(Debug, Deserialize)]
struct OpenAiMessage {
    content: String,
}

#[derive(Debug, Deserialize)]
struct OpenAiChoice {
    message: OpenAiMessage,
}

#[derive(Debug, Deserialize)]
struct OpenAiResponse {
    choices: Vec<OpenAiChoice>,
}

#[derive(Debug, Deserialize)]
struct AnthropicContent {
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContent>,
}

#[derive(Debug, Deserialize)]
struct OllamaResponse {
    response: String,
}

async fn parse_openai_response(response: reqwest::Response) -> Result<String> {
    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        return Err(anyhow!(
            "OpenAI request failed ({status}): {}",
            truncate_body(&body)
        ));
    }
    let parsed: OpenAiResponse =
        serde_json::from_str(&body).context("invalid OpenAI response JSON")?;
    parsed
        .choices
        .into_iter()
        .next()
        .map(|choice| choice.message.content)
        .filter(|content| !content.trim().is_empty())
        .context("OpenAI response missing message content")
}

async fn parse_anthropic_response(response: reqwest::Response) -> Result<String> {
    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        return Err(anyhow!(
            "Anthropic request failed ({status}): {}",
            truncate_body(&body)
        ));
    }
    let parsed: AnthropicResponse =
        serde_json::from_str(&body).context("invalid Anthropic response JSON")?;
    parsed
        .content
        .into_iter()
        .find_map(|part| part.text)
        .filter(|content| !content.trim().is_empty())
        .context("Anthropic response missing text content")
}

async fn parse_ollama_response(response: reqwest::Response) -> Result<String> {
    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        return Err(anyhow!(
            "Ollama request failed ({status}): {}",
            truncate_body(&body)
        ));
    }
    let parsed: OllamaResponse =
        serde_json::from_str(&body).context("invalid Ollama response JSON")?;
    if parsed.response.trim().is_empty() {
        return Err(anyhow!("Ollama response missing generated text"));
    }
    Ok(parsed.response)
}

fn truncate_body(body: &str) -> String {
    const LIMIT: usize = 300;
    if body.len() <= LIMIT {
        body.to_string()
    } else {
        format!("{}...", &body[..LIMIT])
    }
}

mod template_library {
    pub fn match_prompt(prompt: &str) -> String {
        let prompt = prompt.to_ascii_lowercase();
        if prompt.contains("field_mul") {
            return field_mul_template();
        }
        if prompt.contains("field_add") {
            return field_add_template();
        }
        if prompt.contains("verify_proof") {
            return verify_proof_template();
        }
        generic_template()
    }

    fn kani_shim() -> &'static str {
        r#"pub mod kani {
    pub fn any<T: Default>() -> T { T::default() }
    pub fn assume(_cond: bool) {}
    pub fn assert(cond: bool) { assert!(cond); }
}
"#
    }

    fn field_mul_template() -> String {
        format!(
            r#"{shim}
pub fn field_mul(a: u64, b: u64) -> u64 {{
    a.wrapping_mul(b)
}}

pub fn harness() {{
    let a: u64 = kani::any();
    let b: u64 = kani::any();
    let out = field_mul(a, b);
    kani::assert(out == a.wrapping_mul(b));
}}
"#,
            shim = kani_shim()
        )
    }

    fn field_add_template() -> String {
        format!(
            r#"{shim}
pub fn field_add(a: u64, b: u64) -> u64 {{
    a.wrapping_add(b)
}}

pub fn harness() {{
    let a: u64 = kani::any();
    let b: u64 = kani::any();
    let out = field_add(a, b);
    kani::assert(out == a.wrapping_add(b));
}}
"#,
            shim = kani_shim()
        )
    }

    fn verify_proof_template() -> String {
        format!(
            r#"{shim}
pub fn verify_proof(bytes: &[u8]) -> bool {{
    !bytes.is_empty()
}}

pub fn harness() {{
    let input: [u8; 4] = kani::any();
    let _ = verify_proof(&input);
    kani::assert(true);
}}
"#,
            shim = kani_shim()
        )
    }

    fn generic_template() -> String {
        format!(
            r#"{shim}
pub fn harness() {{
    let x: u64 = kani::any();
    kani::assert(x == x);
}}
"#,
            shim = kani_shim()
        )
    }
}
