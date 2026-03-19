use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use audit_agent_core::finding::VerificationStatus;
use llm::copilot::{ChecklistPlan, CopilotService};
use llm::sanitize::{GraphContextEntry, pack_graph_aware_context};
use llm::{CompletionOpts, LlmProvider};

struct CapturePromptProvider {
    prompt: Arc<Mutex<String>>,
    json: String,
}

#[async_trait]
impl LlmProvider for CapturePromptProvider {
    async fn complete(&self, prompt: &str, _opts: &CompletionOpts) -> Result<String> {
        *self.prompt.lock().expect("lock prompt capture") = prompt.to_string();
        Ok(self.json.clone())
    }

    fn name(&self) -> &str {
        "capture"
    }

    fn is_available(&self) -> bool {
        true
    }
}

#[tokio::test]
async fn checklist_plan_parses_structured_json_only() {
    let service = CopilotService::with_mock_json(
        r#"{"domains":[{"id":"crypto","rationale":"key material present"}]}"#,
    );
    let plan: ChecklistPlan = service
        .plan_checklists("rust crypto workspace")
        .await
        .expect("checklist plan");
    assert_eq!(plan.domains[0].id, "crypto");
}

#[tokio::test]
async fn candidate_generation_never_returns_verified_status() {
    let service =
        CopilotService::with_mock_json(r#"{"title":"Possible bug","summary":"review me"}"#);
    let candidate = service
        .generate_candidate("hotspot")
        .await
        .expect("candidate");
    assert!(matches!(
        candidate.verification_status,
        VerificationStatus::Unverified { .. }
    ));
}

#[tokio::test]
async fn candidate_generation_prefers_graph_context_when_available() {
    let captured_prompt = Arc::new(Mutex::new(String::new()));
    let provider = Arc::new(CapturePromptProvider {
        prompt: captured_prompt.clone(),
        json: r#"{"title":"Possible bug","summary":"review me"}"#.to_string(),
    });
    let service = CopilotService::new(provider);

    let candidate = service
        .generate_candidate_with_context(
            "hotspot",
            "SOURCE_CONTEXT_SENTINEL",
            &[GraphContextEntry {
                node_id: "symbol:verify".to_string(),
                content: "GRAPH_CONTEXT_SENTINEL".to_string(),
            }],
        )
        .await
        .expect("candidate");

    assert_eq!(candidate.title, "Possible bug");
    let prompt = captured_prompt.lock().expect("lock captured prompt").clone();
    assert!(prompt.contains("GRAPH_CONTEXT_SENTINEL"));
    assert!(!prompt.contains("SOURCE_CONTEXT_SENTINEL"));
}

#[tokio::test]
async fn candidate_generation_without_graph_context_does_not_duplicate_hotspot() {
    let captured_prompt = Arc::new(Mutex::new(String::new()));
    let provider = Arc::new(CapturePromptProvider {
        prompt: captured_prompt.clone(),
        json: r#"{"title":"Possible bug","summary":"review me"}"#.to_string(),
    });
    let service = CopilotService::new(provider);

    let _candidate = service
        .generate_candidate("HOTSPOT_SENTINEL")
        .await
        .expect("candidate");

    let prompt = captured_prompt.lock().expect("lock captured prompt").clone();
    let count = prompt.matches("HOTSPOT_SENTINEL").count();
    assert_eq!(
        count, 1,
        "hotspot text should appear once when no graph context is supplied"
    );
}

#[test]
fn graph_context_packer_is_deterministic_and_deduplicated() {
    let packed = pack_graph_aware_context(
        "SOURCE_ONLY_SENTINEL",
        &[
            GraphContextEntry {
                node_id: "symbol:z".to_string(),
                content: "z-content".to_string(),
            },
            GraphContextEntry {
                node_id: "symbol:a".to_string(),
                content: "a-content".to_string(),
            },
            GraphContextEntry {
                node_id: "symbol:a".to_string(),
                content: "duplicate-should-not-win".to_string(),
            },
        ],
        300,
    );

    let first_a = packed.find("node=symbol:a").expect("symbol a");
    let first_z = packed.find("node=symbol:z").expect("symbol z");
    assert!(
        first_a < first_z,
        "packer ordering should be deterministic by node id"
    );
    assert!(
        !packed.contains("duplicate-should-not-win"),
        "duplicate entries should be deduplicated"
    );
    assert!(
        !packed.contains("SOURCE_ONLY_SENTINEL"),
        "graph context should be preferred over raw source context"
    );
}

#[test]
fn graph_context_packer_honors_budget_and_falls_back_without_refs() {
    let packed = pack_graph_aware_context(
        "",
        &[GraphContextEntry {
            node_id: "symbol:big".to_string(),
            content: format!(
                "{}{}",
                "B".repeat(512),
                "TRUNCATION_SENTINEL_SHOULD_NOT_APPEAR"
            ),
        }],
        64,
    );
    assert!(packed.chars().count() <= 64);
    assert!(!packed.contains("TRUNCATION_SENTINEL_SHOULD_NOT_APPEAR"));

    let fallback = pack_graph_aware_context("SOURCE_FALLBACK_SENTINEL", &[], 128);
    assert!(fallback.contains("SOURCE_FALLBACK_SENTINEL"));
}

#[test]
fn graph_context_packer_preserves_graph_snippet_formatting() {
    let packed = pack_graph_aware_context(
        "",
        &[GraphContextEntry {
            node_id: "symbol:code".to_string(),
            content: "```rust\nfn verify() {}\n```".to_string(),
        }],
        300,
    );

    assert!(
        packed.contains("```rust"),
        "graph snippet formatting should not be re-sanitized away after packing"
    );
}
