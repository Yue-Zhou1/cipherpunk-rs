use anyhow::Result;
use async_trait::async_trait;
use llm::{CompletionOpts, EvidenceGate, HarnessCode, LlmProvider};
use std::sync::Arc;

struct SyntaxFixProvider;

#[async_trait]
impl LlmProvider for SyntaxFixProvider {
    async fn complete(&self, _prompt: &str, _opts: &CompletionOpts) -> Result<String> {
        Ok("kani::assert!(x > 0);".to_string())
    }

    fn name(&self) -> &str {
        "syntax-fix"
    }

    fn is_available(&self) -> bool {
        true
    }
}

struct CorrectedHarnessProvider;

#[async_trait]
impl LlmProvider for CorrectedHarnessProvider {
    async fn complete(&self, _prompt: &str, _opts: &CompletionOpts) -> Result<String> {
        Ok(r#"
pub mod kani {
    pub fn assert(_cond: bool) {}
}
pub fn harness() {
    let y = 1;
    kani::assert(y > 0);
}
"#
        .to_string())
    }

    fn name(&self) -> &str {
        "corrected"
    }

    fn is_available(&self) -> bool {
        true
    }
}

struct CommentOnlyProvider;

#[async_trait]
impl LlmProvider for CommentOnlyProvider {
    async fn complete(&self, _prompt: &str, _opts: &CompletionOpts) -> Result<String> {
        Ok(r#"
pub mod kani {
    pub fn assert(_cond: bool) {}
}
pub fn harness() {
    // assert! this is only a comment
    let not_assert = "assert! not real";
    kani::assert(true);
}
"#
        .to_string())
    }

    fn name(&self) -> &str {
        "comment-only"
    }

    fn is_available(&self) -> bool {
        true
    }
}

#[test]
fn fix_loop_prompt_forbids_assertion_mutation() {
    let prompt = EvidenceGate::fix_loop_prompt("kani::assert!(x < 10);", "type mismatch");
    assert!(prompt.contains("must remain unchanged"));
    assert!(prompt.contains("Fix syntax/type errors only"));
}

#[tokio::test]
async fn validate_reaches_reproducible_level_for_valid_harness() {
    let gate = EvidenceGate::without_sandbox_for_tests();
    let harness = HarnessCode {
        file_name: "harness.rs".to_string(),
        source: r#"
pub mod kani {
    pub fn any<T: Default>() -> T { T::default() }
    pub fn assume(_cond: bool) {}
    pub fn assert(cond: bool) { assert!(cond); }
}
pub fn harness() {
    let x: u64 = kani::any();
    kani::assume(x < 5);
    let y = x + 1;
    kani::assert(y > 0);
}
"#
        .to_string(),
    };

    let result = gate.validate(&harness, "kani::assert(y > 0);").await;
    assert_eq!(result.level_reached, 3);
    assert!(result.passed);
}

#[tokio::test]
async fn fix_loop_rejects_new_assertions_from_llm() {
    let gate = EvidenceGate::without_sandbox_for_tests();
    let harness = HarnessCode {
        file_name: "harness.rs".to_string(),
        source: "pub fn harness() { let _x = ; }".to_string(),
    };
    let provider = SyntaxFixProvider;

    let result = gate
        .fix_syntax_and_retry(&harness, "expected expression", &provider, 1)
        .await;
    assert!(!result.passed);
    assert!(
        result
            .failure_reason
            .as_deref()
            .unwrap_or("")
            .contains("assertion")
    );
}

#[tokio::test]
async fn fix_loop_uses_real_required_assertion_instead_of_dummy_string() {
    let gate = EvidenceGate::without_sandbox_for_tests();
    let harness = HarnessCode {
        file_name: "harness.rs".to_string(),
        source: r#"
pub fn harness() {
    let y = 1;
    kani::assert(y > 0);
}
"#
        .to_string(),
    };
    let provider = CorrectedHarnessProvider;

    let result = gate
        .fix_syntax_and_retry(&harness, "synthetic compile error", &provider, 1)
        .await;
    assert!(
        result.passed,
        "fix loop should validate against the rule-derived assertion, not a dummy literal: {result:?}"
    );
    let provenance = result
        .provenance
        .as_ref()
        .expect("fix-loop result should carry llm provenance");
    assert_eq!(provenance.provider, "corrected");
    assert_eq!(provenance.role, "Scaffolding");
    assert_eq!(provenance.attempt, 1);
}

#[tokio::test]
async fn assertion_counter_ignores_comment_and_string_literals() {
    let gate = EvidenceGate::without_sandbox_for_tests();
    let harness = HarnessCode {
        file_name: "harness.rs".to_string(),
        source: r#"
pub fn harness() {
    kani::assert(true);
    let x = ;
}
"#
        .to_string(),
    };
    let provider = CommentOnlyProvider;

    let result = gate
        .fix_syntax_and_retry(&harness, "expected expression", &provider, 1)
        .await;
    assert!(
        result.level_reached >= 1,
        "comment/string mentions of assert should not trigger assertion-mutation blocking: {result:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn validate_runs_concurrently_without_binary_path_collisions() {
    let gate = Arc::new(EvidenceGate::without_sandbox_for_tests());
    let harness = HarnessCode {
        file_name: "harness.rs".to_string(),
        source: r#"
pub mod kani {
    pub fn any<T: Default>() -> T { T::default() }
    pub fn assume(_cond: bool) {}
    pub fn assert(cond: bool) { assert!(cond); }
}
pub fn harness() {
    let x: u64 = kani::any();
    kani::assume(x < 5);
    let y = x + 1;
    kani::assert(y > 0);
}
"#
        .to_string(),
    };

    let tasks: Vec<_> = (0..24)
        .map(|_| {
            let gate = gate.clone();
            let harness = harness.clone();
            tokio::spawn(async move { gate.validate(&harness, "kani::assert(y > 0);").await })
        })
        .collect();

    for task in tasks {
        let result = task.await.expect("join validate task");
        assert!(result.passed, "concurrent validate should pass: {result:?}");
    }
}
