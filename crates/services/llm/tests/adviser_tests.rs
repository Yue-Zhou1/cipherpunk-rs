use std::collections::VecDeque;
use std::sync::Mutex;

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use llm::{
    AdviserAction, AdviserBudgetSnapshot, AdviserContext, AdviserService, CompletionOpts,
    LlmProvider,
};

#[derive(Debug)]
struct SequenceProvider {
    responses: Mutex<VecDeque<Result<String>>>,
}

impl SequenceProvider {
    fn new(responses: Vec<Result<String>>) -> Self {
        Self {
            responses: Mutex::new(responses.into()),
        }
    }
}

#[async_trait]
impl LlmProvider for SequenceProvider {
    async fn complete(&self, _prompt: &str, _opts: &CompletionOpts) -> Result<String> {
        self.responses
            .lock()
            .expect("responses lock")
            .pop_front()
            .unwrap_or_else(|| Err(anyhow!("missing response")))
    }

    fn name(&self) -> &str {
        "sequence"
    }

    fn is_available(&self) -> bool {
        true
    }

    fn model(&self) -> Option<&str> {
        Some("sequence-v1")
    }
}

fn context() -> AdviserContext {
    AdviserContext {
        engine_name: "z3-engine".to_string(),
        error_message: "timeout while solving constraints".to_string(),
        attempt_number: 1,
        elapsed_ms: 1_500,
        findings_so_far: 2,
        budget: AdviserBudgetSnapshot {
            timeout_secs: 60,
            memory_mb: 1024,
            cpu_cores: 2.0,
        },
    }
}

#[tokio::test]
async fn adviser_service_parses_retry_suggestion_json() {
    let provider = SequenceProvider::new(vec![Ok(
        r#"{"action":{"type":"RetryWithRelaxedBudget","timeout_secs":600,"memory_mb":2048},"rationale":"z3 timed out under current budget"}"#
            .to_string(),
    )]);
    let service = AdviserService::new(std::sync::Arc::new(provider));

    let suggestion = service
        .suggest_on_failure(&context())
        .await
        .expect("adviser suggestion");

    match suggestion.action {
        AdviserAction::RetryWithRelaxedBudget {
            timeout_secs,
            memory_mb,
        } => {
            assert_eq!(timeout_secs, 600);
            assert_eq!(memory_mb, 2048);
        }
        other => panic!("expected RetryWithRelaxedBudget, got {other:?}"),
    }
    assert!(suggestion.rationale.contains("timed out"));
}

#[tokio::test]
async fn adviser_service_falls_back_to_no_suggestion_on_invalid_json() {
    let provider = SequenceProvider::new(vec![Ok("not-json".to_string())]);
    let service = AdviserService::new(std::sync::Arc::new(provider));

    let suggestion = service
        .suggest_on_failure(&context())
        .await
        .expect("fallback suggestion");

    assert!(matches!(suggestion.action, AdviserAction::NoSuggestion));
    assert!(suggestion.rationale.contains("could not produce"));
}

#[tokio::test]
async fn adviser_service_handles_utf8_error_messages_without_panicking() {
    let provider = SequenceProvider::new(vec![Ok(
        r#"{"action":{"type":"NoSuggestion"},"rationale":"no recovery"}"#.to_string(),
    )]);
    let service = AdviserService::new(std::sync::Arc::new(provider));

    let mut context = context();
    context.error_message = "€".repeat(600);

    let suggestion = service
        .suggest_on_failure(&context)
        .await
        .expect("adviser should tolerate utf-8 truncation");

    assert!(matches!(suggestion.action, AdviserAction::NoSuggestion));
}
