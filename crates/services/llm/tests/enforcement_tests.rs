use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use llm::enforcement::{ContractEnforcer, RetryPolicy, retry_policy_for_role};
use llm::{CompletionOpts, LlmProvider, LlmRole};
use serde::Deserialize;

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

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
struct DemoContract {
    value: String,
}

#[tokio::test]
async fn enforcer_accepts_valid_json_on_first_attempt() {
    let provider = SequenceProvider::new(vec![Ok(r#"{"value":"ok"}"#.to_string())]);
    let enforcer = ContractEnforcer::<DemoContract>::new(LlmRole::SearchHints, "DemoContract")
        .with_retry(RetryPolicy {
            max_attempts: 3,
            backoff_ms: 0,
        });

    let response = enforcer
        .execute(
            &provider,
            "return demo json",
            &CompletionOpts::default(),
            None,
        )
        .await
        .expect("enforce contract");

    assert_eq!(
        response.value,
        DemoContract {
            value: "ok".to_string()
        }
    );
    assert_eq!(response.provenance.attempt, 1);
}

#[tokio::test]
async fn enforcer_retries_invalid_json_and_returns_second_attempt() {
    let provider = SequenceProvider::new(vec![
        Ok("not-json".to_string()),
        Ok(r#"{"value":"recovered"}"#.to_string()),
    ]);
    let enforcer = ContractEnforcer::<DemoContract>::new(LlmRole::SearchHints, "DemoContract")
        .with_retry(RetryPolicy {
            max_attempts: 2,
            backoff_ms: 0,
        });

    let response = enforcer
        .execute(
            &provider,
            "return demo json",
            &CompletionOpts::default(),
            None,
        )
        .await
        .expect("enforce contract");

    assert_eq!(response.value.value, "recovered");
    assert_eq!(response.provenance.attempt, 2);
}

#[tokio::test]
async fn enforcer_uses_fallback_when_retries_exhausted() {
    let provider = SequenceProvider::new(vec![Ok("bad".to_string()), Ok("still-bad".to_string())]);
    let enforcer = ContractEnforcer::<DemoContract>::new(LlmRole::SearchHints, "DemoContract")
        .with_retry(RetryPolicy {
            max_attempts: 2,
            backoff_ms: 0,
        })
        .with_fallback(DemoContract {
            value: "fallback".to_string(),
        });

    let response = enforcer
        .execute(
            &provider,
            "return demo json",
            &CompletionOpts::default(),
            None,
        )
        .await
        .expect("fallback response");

    assert_eq!(response.value.value, "fallback");
    assert_eq!(response.provenance.provider, "fallback");
    assert_eq!(response.provenance.attempt, 2);
}

#[tokio::test]
async fn enforcer_errors_when_retries_exhausted_without_fallback() {
    let provider = SequenceProvider::new(vec![Ok("bad".to_string()), Ok("still-bad".to_string())]);
    let enforcer = ContractEnforcer::<DemoContract>::new(LlmRole::SearchHints, "DemoContract")
        .with_retry(RetryPolicy {
            max_attempts: 2,
            backoff_ms: 0,
        });

    let error = enforcer
        .execute(
            &provider,
            "return demo json",
            &CompletionOpts::default(),
            None,
        )
        .await
        .expect_err("should fail");

    assert!(error.to_string().contains("contract enforcement failed"));
}

#[test]
fn retry_policy_matches_role_defaults() {
    let scaffolding = retry_policy_for_role(&LlmRole::Scaffolding);
    assert_eq!(scaffolding.max_attempts, 3);
    assert_eq!(scaffolding.backoff_ms, 1_000);

    let prose = retry_policy_for_role(&LlmRole::ProseRendering);
    assert_eq!(prose.max_attempts, 1);
    assert_eq!(prose.backoff_ms, 0);

    let advisory = retry_policy_for_role(&LlmRole::Advisory);
    assert_eq!(advisory.max_attempts, 1);
    assert_eq!(advisory.backoff_ms, 0);
}

#[tokio::test]
async fn enforcer_emits_interaction_hook_events() {
    let provider = SequenceProvider::new(vec![Ok(r#"{"value":"ok"}"#.to_string())]);
    let captured = Arc::new(Mutex::new(Vec::<(String, bool, u8)>::new()));
    let captured_hook = Arc::clone(&captured);
    let hook: llm::LlmInteractionHook =
        Arc::new(move |provenance: &llm::LlmProvenance, succeeded: bool| {
            captured_hook.lock().expect("capture lock").push((
                provenance.provider.clone(),
                succeeded,
                provenance.attempt,
            ));
        });
    let enforcer = ContractEnforcer::<DemoContract>::new(LlmRole::SearchHints, "DemoContract")
        .with_retry(RetryPolicy {
            max_attempts: 1,
            backoff_ms: 0,
        });

    let _response = enforcer
        .execute(
            &provider,
            "return demo json",
            &CompletionOpts::default(),
            Some(&hook),
        )
        .await
        .expect("enforce contract");

    let events = captured.lock().expect("capture lock");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].0, "sequence");
    assert!(events[0].1);
    assert_eq!(events[0].2, 1);
}
