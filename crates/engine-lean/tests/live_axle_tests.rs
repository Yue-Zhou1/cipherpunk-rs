//! Live integration tests against the real AXLE server.
//!
//! These tests require network access and are excluded from normal CI runs.
//! Run them with:
//!
//!   # Anonymous (no key, reduced concurrency):
//!   cargo test -p engine-lean -- --ignored live_
//!
//!   # Authenticated (higher concurrency):
//!   AXLE_API_KEY=<your-key> cargo test -p engine-lean -- --ignored live_

use audit_agent_core::tooling::{
    ToolActionRequest, ToolActionStatus, ToolBudget, ToolFamily, ToolTarget,
};
use engine_lean::client::AxleClient;
use engine_lean::tool_actions::axle::execute_lean_action;
use engine_lean::types::{
    AXLE_BASE_URL, AxleCheckRequest, AxleDisproveRequest, AxleSorry2LemmaRequest, DEFAULT_LEAN_ENV,
};
use std::io::Write;
use tempfile::NamedTempFile;

const VALID_LEAN: &str = r#"
import Mathlib

theorem addition_comm (n m : Nat) : n + m = m + n := Nat.add_comm n m
"#;

const LEAN_WITH_SORRY: &str = r#"
import Mathlib

theorem mul_comm_sorry (n m : Nat) : n * m = m * n := by sorry
"#;

const FALSE_CLAIM: &str = r#"
import Mathlib

theorem false_addition : 1 + 1 = 3 := by sorry
"#;

const INVALID_LEAN: &str = r#"
import Mathlib

theorem broken : @@@INVALID@@@ := by decide
"#;

fn make_client() -> AxleClient {
    AxleClient::from_env(AXLE_BASE_URL.to_string())
}

#[tokio::test]
#[ignore = "requires live network access to axle.axiommath.ai"]
async fn live_check_valid_lean_returns_okay_true() {
    let client = make_client();
    let result = client
        .check(&AxleCheckRequest {
            content: VALID_LEAN.to_string(),
            environment: DEFAULT_LEAN_ENV.to_string(),
            timeout_seconds: Some(120.0),
        })
        .await
        .expect("AXLE /check request must succeed");

    assert!(
        result.okay,
        "valid Lean should compile cleanly; errors: {:?}",
        result.lean_messages.errors
    );
    assert!(result.lean_messages.errors.is_empty());
    assert!(result.failed_declarations.is_empty());
}

#[tokio::test]
#[ignore = "requires live network access to axle.axiommath.ai"]
async fn live_check_invalid_lean_returns_okay_false_with_errors() {
    let client = make_client();
    let result = client
        .check(&AxleCheckRequest {
            content: INVALID_LEAN.to_string(),
            environment: DEFAULT_LEAN_ENV.to_string(),
            timeout_seconds: Some(120.0),
        })
        .await
        .expect("AXLE /check request must succeed (HTTP 200 even for invalid Lean)");

    assert!(
        !result.okay,
        "invalid Lean must not compile; got okay=true unexpectedly"
    );
    assert!(
        !result.lean_messages.errors.is_empty(),
        "invalid Lean must produce at least one error message"
    );
}

#[tokio::test]
#[ignore = "requires live network access to axle.axiommath.ai"]
async fn live_sorry2lemma_extracts_at_least_one_lemma() {
    let client = make_client();
    let result = client
        .sorry2lemma(&AxleSorry2LemmaRequest {
            content: LEAN_WITH_SORRY.to_string(),
            environment: DEFAULT_LEAN_ENV.to_string(),
            extract_sorries: Some(true),
            extract_errors: Some(false),
            timeout_seconds: Some(120.0),
        })
        .await
        .expect("AXLE /sorry2lemma request must succeed");

    assert!(
        !result.lemma_names.is_empty(),
        "sorry2lemma must extract at least one lemma from the sorry stub; got none.\n\
         lean_messages: {:?}",
        result.lean_messages.errors
    );
    assert!(
        result.content.contains("mul_comm_sorry"),
        "extracted content must reference the original theorem name"
    );
}

#[tokio::test]
#[ignore = "requires live network access to axle.axiommath.ai"]
async fn live_disprove_finds_counterexample_for_false_claim() {
    let client = make_client();
    let result = client
        .disprove(&AxleDisproveRequest {
            content: FALSE_CLAIM.to_string(),
            environment: DEFAULT_LEAN_ENV.to_string(),
            names: Some(vec!["false_addition".to_string()]),
            timeout_seconds: Some(120.0),
        })
        .await
        .expect("AXLE /disprove request must succeed");

    assert!(
        result
            .disproved_theorems
            .contains(&"false_addition".to_string()),
        "AXLE must disprove the false 1+1=3 claim; disproved={:?}, errors={:?}",
        result.disproved_theorems,
        result.lean_messages.errors
    );
}

#[tokio::test]
#[ignore = "requires live network access to axle.axiommath.ai"]
async fn live_disprove_does_not_disprove_true_theorem() {
    let client = make_client();
    let result = client
        .disprove(&AxleDisproveRequest {
            content: VALID_LEAN.to_string(),
            environment: DEFAULT_LEAN_ENV.to_string(),
            names: Some(vec!["addition_comm".to_string()]),
            timeout_seconds: Some(120.0),
        })
        .await
        .expect("AXLE /disprove request must succeed");

    assert!(
        result.disproved_theorems.is_empty(),
        "AXLE must not disprove a true theorem; unexpectedly disproved={:?}",
        result.disproved_theorems
    );
}

#[tokio::test]
#[ignore = "requires live network access to axle.axiommath.ai"]
async fn live_full_pipeline_on_sorry_theorem_completes() {
    let artifact_root = tempfile::tempdir().unwrap();
    let mut tmp = NamedTempFile::new().unwrap();
    tmp.write_all(LEAN_WITH_SORRY.as_bytes()).unwrap();
    let path = tmp.path().to_str().unwrap().to_string();

    let request = ToolActionRequest {
        session_id: "live-test-session".to_string(),
        workspace_root: None,
        tool_family: ToolFamily::LeanExternal,
        target: ToolTarget::File { path },
        budget: ToolBudget {
            timeout_secs: 360,
            ..ToolBudget::default()
        },
    };

    let result = execute_lean_action(&request, AXLE_BASE_URL, artifact_root.path())
        .await
        .expect("full pipeline must complete without error");

    assert_eq!(
        result.status,
        ToolActionStatus::Completed,
        "pipeline must complete; preview={:?}",
        result.stdout_preview
    );
    assert!(!result.artifact_refs.is_empty());

    let preview = result.stdout_preview.unwrap();
    assert!(preview.contains("check: ok"), "preview: {preview}");
    assert!(preview.contains("lemmas extracted:"), "preview: {preview}");
    eprintln!("Live pipeline result:\n{preview}");
}

#[tokio::test]
#[ignore = "requires live network access to axle.axiommath.ai"]
async fn live_full_pipeline_on_false_claim_completes_and_reports_disproved() {
    let artifact_root = tempfile::tempdir().unwrap();
    let mut tmp = NamedTempFile::new().unwrap();
    tmp.write_all(FALSE_CLAIM.as_bytes()).unwrap();
    let path = tmp.path().to_str().unwrap().to_string();

    let request = ToolActionRequest {
        session_id: "live-test-session-disprove".to_string(),
        workspace_root: None,
        tool_family: ToolFamily::LeanExternal,
        target: ToolTarget::File { path },
        budget: ToolBudget {
            timeout_secs: 360,
            ..ToolBudget::default()
        },
    };

    let result = execute_lean_action(&request, AXLE_BASE_URL, artifact_root.path())
        .await
        .expect("full pipeline must complete without error");

    assert_eq!(result.status, ToolActionStatus::Completed);
    let preview = result.stdout_preview.unwrap();
    assert!(
        !preview.contains("disproved: none"),
        "false_addition must be disproved; preview: {preview}"
    );
    eprintln!("Live disprove pipeline result:\n{preview}");
}
