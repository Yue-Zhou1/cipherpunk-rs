#![allow(deprecated)]

use anyhow::Result;
use async_trait::async_trait;
use llm::{
    CompletionOpts, LlmProvider, LlmRole, TemplateFallback, llm_call, llm_call_traced,
    provider_from_env, provider_from_name,
};
use mockito::Matcher;
use std::process::Command;
use std::sync::{Mutex, OnceLock};

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

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

    fn model(&self) -> Option<&str> {
        Some("echo-v1")
    }
}

#[test]
fn llm_api_key_absent_uses_template_fallback_silently() {
    let _guard = env_lock().lock().expect("env lock");
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
async fn llm_call_traced_returns_response_with_provenance() {
    let provider = EchoProvider;
    let (output, provenance) = llm_call_traced(
        &provider,
        LlmRole::SearchHints,
        "hello",
        &CompletionOpts::default(),
    )
    .await
    .expect("llm traced call");
    assert_eq!(output, "echo:hello");
    assert_eq!(provenance.provider, "echo");
    assert_eq!(provenance.model.as_deref(), Some("echo-v1"));
    assert_eq!(provenance.role, "SearchHints");
    assert_eq!(provenance.prompt_chars, 5);
    assert_eq!(provenance.response_chars, "echo:hello".len());
    assert_eq!(provenance.attempt, 1);
}

#[tokio::test]
async fn openai_provider_makes_chat_completion_request() {
    let mut server = mockito::Server::new_async().await;
    let request = server
        .mock("POST", "/v1/chat/completions")
        .match_header("authorization", "Bearer key")
        .match_body(Matcher::PartialJson(serde_json::json!({
            "model": "gpt-4o-mini",
            "messages": [{"role": "user", "content": "hello"}],
            "max_tokens": 32
        })))
        .with_status(200)
        .with_body(r#"{"choices":[{"message":{"content":"openai-ok"}}]}"#)
        .create_async()
        .await;

    let openai =
        llm::OpenAiProvider::new("key".to_string(), "gpt-4o-mini".to_string(), server.url())
            .expect("openai provider");
    let response = openai
        .complete(
            "hello",
            &CompletionOpts {
                temperature_millis: 100,
                max_tokens: 32,
            },
        )
        .await
        .expect("openai response");
    assert_eq!(response, "openai-ok");
    request.assert_async().await;
}

#[tokio::test]
async fn openai_provider_complete_with_role_and_model_overrides_model() {
    let mut server = mockito::Server::new_async().await;
    let request = server
        .mock("POST", "/v1/chat/completions")
        .match_header("authorization", "Bearer key")
        .match_body(Matcher::PartialJson(serde_json::json!({
            "model": "gpt-4.1",
            "messages": [{"role": "user", "content": "hello"}],
            "max_tokens": 32
        })))
        .with_status(200)
        .with_body(r#"{"choices":[{"message":{"content":"openai-ok"}}]}"#)
        .create_async()
        .await;

    let openai =
        llm::OpenAiProvider::new("key".to_string(), "gpt-4o-mini".to_string(), server.url())
            .expect("openai provider");
    let output = LlmProvider::complete_with_role_and_model(
        &openai,
        &LlmRole::Scaffolding,
        "hello",
        &CompletionOpts {
            temperature_millis: 100,
            max_tokens: 32,
        },
        Some("gpt-4.1"),
    )
    .await
    .expect("openai response");
    assert_eq!(output.response, "openai-ok");
    assert_eq!(output.model.as_deref(), Some("gpt-4.1"));
    request.assert_async().await;
}

#[tokio::test]
async fn anthropic_provider_makes_messages_request() {
    let mut server = mockito::Server::new_async().await;
    let request = server
        .mock("POST", "/v1/messages")
        .match_header("x-api-key", "key")
        .match_header("anthropic-version", "2023-06-01")
        .match_body(Matcher::PartialJson(serde_json::json!({
            "model": "claude-3-5-sonnet",
            "messages": [{"role": "user", "content": "hello"}],
            "max_tokens": 48
        })))
        .with_status(200)
        .with_body(r#"{"content":[{"type":"text","text":"anthropic-ok"}]}"#)
        .create_async()
        .await;

    let anthropic = llm::AnthropicProvider::new(
        "key".to_string(),
        "claude-3-5-sonnet".to_string(),
        server.url(),
    )
    .expect("anthropic provider");
    let response = anthropic
        .complete(
            "hello",
            &CompletionOpts {
                temperature_millis: 200,
                max_tokens: 48,
            },
        )
        .await
        .expect("anthropic response");
    assert_eq!(response, "anthropic-ok");
    request.assert_async().await;
}

#[tokio::test]
async fn anthropic_provider_complete_with_role_and_model_overrides_model() {
    let mut server = mockito::Server::new_async().await;
    let request = server
        .mock("POST", "/v1/messages")
        .match_header("x-api-key", "key")
        .match_header("anthropic-version", "2023-06-01")
        .match_body(Matcher::PartialJson(serde_json::json!({
            "model": "claude-3-7-sonnet",
            "messages": [{"role": "user", "content": "hello"}],
            "max_tokens": 48
        })))
        .with_status(200)
        .with_body(r#"{"content":[{"type":"text","text":"anthropic-ok"}]}"#)
        .create_async()
        .await;

    let anthropic = llm::AnthropicProvider::new(
        "key".to_string(),
        "claude-3-5-sonnet".to_string(),
        server.url(),
    )
    .expect("anthropic provider");
    let output = LlmProvider::complete_with_role_and_model(
        &anthropic,
        &LlmRole::SearchHints,
        "hello",
        &CompletionOpts {
            temperature_millis: 200,
            max_tokens: 48,
        },
        Some("claude-3-7-sonnet"),
    )
    .await
    .expect("anthropic response");
    assert_eq!(output.response, "anthropic-ok");
    assert_eq!(output.model.as_deref(), Some("claude-3-7-sonnet"));
    request.assert_async().await;
}

#[tokio::test]
async fn ollama_provider_makes_generate_request() {
    let mut server = mockito::Server::new_async().await;
    let request = server
        .mock("POST", "/api/generate")
        .match_body(Matcher::PartialJson(serde_json::json!({
            "model": "llama3",
            "prompt": "hello",
            "stream": false
        })))
        .with_status(200)
        .with_body(r#"{"response":"ollama-ok"}"#)
        .create_async()
        .await;

    let ollama =
        llm::OllamaProvider::new(server.url(), "llama3".to_string()).expect("ollama provider");
    let response = ollama
        .complete(
            "hello",
            &CompletionOpts {
                temperature_millis: 350,
                max_tokens: 24,
            },
        )
        .await
        .expect("ollama response");
    assert_eq!(response, "ollama-ok");
    request.assert_async().await;
}

#[tokio::test]
async fn ollama_provider_complete_with_role_and_model_overrides_model() {
    let mut server = mockito::Server::new_async().await;
    let request = server
        .mock("POST", "/api/generate")
        .match_body(Matcher::PartialJson(serde_json::json!({
            "model": "qwen2.5-coder",
            "prompt": "hello",
            "stream": false
        })))
        .with_status(200)
        .with_body(r#"{"response":"ollama-ok"}"#)
        .create_async()
        .await;

    let ollama =
        llm::OllamaProvider::new(server.url(), "llama3".to_string()).expect("ollama provider");
    let output = LlmProvider::complete_with_role_and_model(
        &ollama,
        &LlmRole::ProseRendering,
        "hello",
        &CompletionOpts {
            temperature_millis: 350,
            max_tokens: 24,
        },
        Some("qwen2.5-coder"),
    )
    .await
    .expect("ollama response");
    assert_eq!(output.response, "ollama-ok");
    assert_eq!(output.model.as_deref(), Some("qwen2.5-coder"));
    request.assert_async().await;
}

#[test]
fn provider_selection_respects_requested_backend() {
    let _guard = env_lock().lock().expect("env lock");
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
    let _guard = env_lock().lock().expect("env lock");
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

#[test]
fn provider_from_name_supports_template_aliases() {
    assert_eq!(provider_from_name("template").name(), "template-fallback");
    assert_eq!(
        provider_from_name("template-fallback").name(),
        "template-fallback"
    );
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
