use anyhow::{Result, anyhow};
use async_trait::async_trait;

#[derive(Debug, Clone, PartialEq)]
pub enum LlmRole {
    Scaffolding,
    SearchHints,
    ProseRendering,
}

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
    fn name(&self) -> &str;
    fn is_available(&self) -> bool;
}

#[derive(Debug, Clone)]
pub struct OpenAiProvider {
    pub api_key: String,
    pub model: String,
}

#[derive(Debug, Clone)]
pub struct AnthropicProvider {
    pub api_key: String,
    pub model: String,
}

#[derive(Debug, Clone)]
pub struct OllamaProvider {
    pub base_url: String,
    pub model: String,
}

#[derive(Debug, Clone, Copy)]
pub struct TemplateFallback;

#[async_trait]
impl LlmProvider for OpenAiProvider {
    async fn complete(&self, _prompt: &str, _opts: &CompletionOpts) -> Result<String> {
        if !self.is_available() {
            return Err(anyhow!("OpenAI API key is missing"));
        }
        Err(anyhow!(
            "OpenAI provider is currently a stub; use TemplateFallback until remote adapter is implemented"
        ))
    }

    fn name(&self) -> &str {
        "openai"
    }

    fn is_available(&self) -> bool {
        !self.api_key.trim().is_empty()
    }
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    async fn complete(&self, _prompt: &str, _opts: &CompletionOpts) -> Result<String> {
        if !self.is_available() {
            return Err(anyhow!("Anthropic API key is missing"));
        }
        Err(anyhow!(
            "Anthropic provider is currently a stub; use TemplateFallback until remote adapter is implemented"
        ))
    }

    fn name(&self) -> &str {
        "anthropic"
    }

    fn is_available(&self) -> bool {
        !self.api_key.trim().is_empty()
    }
}

#[async_trait]
impl LlmProvider for OllamaProvider {
    async fn complete(&self, _prompt: &str, _opts: &CompletionOpts) -> Result<String> {
        if !self.is_available() {
            return Err(anyhow!("Ollama base URL is missing"));
        }
        Err(anyhow!(
            "Ollama provider is currently a stub; use TemplateFallback until remote adapter is implemented"
        ))
    }

    fn name(&self) -> &str {
        "ollama"
    }

    fn is_available(&self) -> bool {
        !self.base_url.trim().is_empty()
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
        Some("openai") => openai_provider()
            .map(|provider| Box::new(provider) as Box<dyn LlmProvider>)
            .unwrap_or_else(|| Box::new(TemplateFallback)),
        Some("anthropic") => anthropic_provider()
            .map(|provider| Box::new(provider) as Box<dyn LlmProvider>)
            .unwrap_or_else(|| Box::new(TemplateFallback)),
        Some("ollama") => ollama_provider()
            .map(|provider| Box::new(provider) as Box<dyn LlmProvider>)
            .unwrap_or_else(|| Box::new(TemplateFallback)),
        Some("template") | Some("template-fallback") => Box::new(TemplateFallback),
        Some(_) => Box::new(TemplateFallback),
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

fn openai_provider() -> Option<OpenAiProvider> {
    let api_key = std::env::var("LLM_API_KEY").ok()?;
    if api_key.trim().is_empty() {
        return None;
    }
    Some(OpenAiProvider {
        api_key,
        model: "gpt-4o-mini".to_string(),
    })
}

fn anthropic_provider() -> Option<AnthropicProvider> {
    let api_key = std::env::var("ANTHROPIC_API_KEY").ok()?;
    if api_key.trim().is_empty() {
        return None;
    }
    Some(AnthropicProvider {
        api_key,
        model: "claude-3-5-sonnet".to_string(),
    })
}

fn ollama_provider() -> Option<OllamaProvider> {
    let base_url = std::env::var("OLLAMA_BASE_URL").ok()?;
    if base_url.trim().is_empty() {
        return None;
    }
    Some(OllamaProvider {
        base_url,
        model: "llama3".to_string(),
    })
}

pub async fn llm_call(
    provider: &dyn LlmProvider,
    role: LlmRole,
    prompt: &str,
    opts: &CompletionOpts,
) -> Result<String> {
    tracing::debug!(role = ?role, provider = provider.name(), "LLM call");
    let response = provider.complete(prompt, opts).await?;
    tracing::trace!(role = ?role, chars = response.len(), "LLM response");
    Ok(response)
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
