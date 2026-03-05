use anyhow::Result;
use async_trait::async_trait;
use llm::{CompletionOpts, LlmProvider, LlmRole, TemplateFallback, llm_call, provider_from_env};
use std::process::Command;

struct EchoProvider;

#[async_trait]
impl LlmProvider for EchoProvider {
    async fn complete(&self, prompt: &str, _opts: &CompletionOpts) -> Result<String> {
        Ok(format!("echo:{prompt}"))
    }

    fn name(&self) -> &str {
        "echo"
    }

    fn is_available(&self) -> bool {
        true
    }
}

#[test]
fn llm_api_key_absent_uses_template_fallback_silently() {
    let previous = std::env::var("LLM_API_KEY").ok();
    // SAFETY: test is single-threaded with respect to this env var.
    unsafe { std::env::remove_var("LLM_API_KEY") };

    let provider = provider_from_env();
    assert_eq!(provider.name(), "template-fallback");
    assert!(provider.is_available());

    if let Some(value) = previous {
        // SAFETY: restoring test-local env var.
        unsafe { std::env::set_var("LLM_API_KEY", value) };
    }
}

#[tokio::test]
async fn template_fallback_returns_compilable_harness_templates() {
    let provider = TemplateFallback;
    let opts = CompletionOpts::default();

    let field_mul = provider
        .complete("generate harness for field_mul", &opts)
        .await
        .expect("template fallback response");
    let field_add = provider
        .complete("generate harness for field_add", &opts)
        .await
        .expect("template fallback response");
    let verify = provider
        .complete("generate harness for verify_proof", &opts)
        .await
        .expect("template fallback response");

    for output in [&field_mul, &field_add, &verify] {
        assert!(
            output.contains("fn harness"),
            "template must include harness fn"
        );
        assert!(
            output.contains("kani::"),
            "template should include kani helpers for scaffolding"
        );
        compile_snippet(output);
    }
}

#[tokio::test]
async fn llm_call_routes_through_provider() {
    let provider = EchoProvider;
    let output = llm_call(
        &provider,
        LlmRole::SearchHints,
        "hello",
        &CompletionOpts::default(),
    )
    .await
    .expect("llm call");
    assert_eq!(output, "echo:hello");
}

#[tokio::test]
async fn concrete_remote_providers_are_explicitly_stubbed() {
    let openai = llm::OpenAiProvider {
        api_key: "key".to_string(),
        model: "gpt-4o-mini".to_string(),
    };
    let anthropic = llm::AnthropicProvider {
        api_key: "key".to_string(),
        model: "claude-3-5-sonnet".to_string(),
    };
    let ollama = llm::OllamaProvider {
        base_url: "http://localhost:11434".to_string(),
        model: "llama3".to_string(),
    };
    for (name, provider) in [
        ("openai", &openai as &dyn LlmProvider),
        ("anthropic", &anthropic as &dyn LlmProvider),
        ("ollama", &ollama as &dyn LlmProvider),
    ] {
        let err = provider
            .complete("hello", &CompletionOpts::default())
            .await
            .expect_err("provider should be marked as stub");
        assert!(
            err.to_string().to_ascii_lowercase().contains("stub"),
            "{name} provider must explicitly declare stub status"
        );
    }
}

#[test]
fn provider_selection_respects_requested_backend() {
    let restore = snapshot_env(&[
        "LLM_PROVIDER",
        "LLM_API_KEY",
        "ANTHROPIC_API_KEY",
        "OLLAMA_BASE_URL",
    ]);

    // SAFETY: test-local environment setup.
    unsafe {
        std::env::set_var("LLM_PROVIDER", "anthropic");
        std::env::set_var("ANTHROPIC_API_KEY", "anthropic-key");
        std::env::remove_var("LLM_API_KEY");
        std::env::remove_var("OLLAMA_BASE_URL");
    }
    assert_eq!(provider_from_env().name(), "anthropic");

    // SAFETY: test-local environment setup.
    unsafe {
        std::env::set_var("LLM_PROVIDER", "ollama");
        std::env::set_var("OLLAMA_BASE_URL", "http://localhost:11434");
        std::env::remove_var("ANTHROPIC_API_KEY");
    }
    assert_eq!(provider_from_env().name(), "ollama");

    // SAFETY: test-local environment setup.
    unsafe {
        std::env::set_var("LLM_PROVIDER", "openai");
        std::env::set_var("LLM_API_KEY", "openai-key");
        std::env::remove_var("ANTHROPIC_API_KEY");
        std::env::remove_var("OLLAMA_BASE_URL");
    }
    assert_eq!(provider_from_env().name(), "openai");

    restore_env(restore);
}

#[test]
fn provider_selection_falls_back_to_template_when_requested_provider_unavailable() {
    let restore = snapshot_env(&[
        "LLM_PROVIDER",
        "LLM_API_KEY",
        "ANTHROPIC_API_KEY",
        "OLLAMA_BASE_URL",
    ]);
    // SAFETY: test-local environment setup.
    unsafe {
        std::env::set_var("LLM_PROVIDER", "anthropic");
        std::env::remove_var("ANTHROPIC_API_KEY");
        std::env::remove_var("LLM_API_KEY");
        std::env::remove_var("OLLAMA_BASE_URL");
    }

    assert_eq!(provider_from_env().name(), "template-fallback");
    restore_env(restore);
}

fn compile_snippet(source: &str) {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("snippet.rs");
    let mut wrapped = source.to_string();
    wrapped.push_str("\nfn main() { harness(); }\n");
    std::fs::write(&file, wrapped).expect("write snippet");

    let output = Command::new("rustc")
        .arg("--edition=2024")
        .arg(&file)
        .arg("-o")
        .arg(dir.path().join("snippet-bin"))
        .output()
        .expect("run rustc");
    assert!(
        output.status.success(),
        "template must compile, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn snapshot_env(keys: &[&str]) -> Vec<(String, Option<String>)> {
    keys.iter()
        .map(|k| ((*k).to_string(), std::env::var(k).ok()))
        .collect()
}

fn restore_env(state: Vec<(String, Option<String>)>) {
    for (key, value) in state {
        match value {
            Some(v) => {
                // SAFETY: restoring test-local env vars.
                unsafe { std::env::set_var(key, v) };
            }
            None => {
                // SAFETY: restoring test-local env vars.
                unsafe { std::env::remove_var(key) };
            }
        }
    }
}
