use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use audit_agent_core::finding::CodeLocation;
use engine_crypto::kani::scaffolder::{
    AssertionSpec, FunctionSignature, HarnessRequest, KaniHarnessScaffolder, RuleTrigger,
    parse_assume_lines,
};
use llm::{CompletionOpts, EvidenceGate, LlmProvider};
use llm::sanitize::GraphContextEntry;
use num_bigint::BigUint;

struct AdversarialHintProvider;

#[async_trait]
impl LlmProvider for AdversarialHintProvider {
    async fn complete(&self, _prompt: &str, _opts: &CompletionOpts) -> Result<String> {
        Ok(r#"
kani::assume!(a < 10);
println!("oops");
kani::assume!(false);
kani::assume!(1 == 0);
kani::assume!(a % 2 == 0);
kani::assume!(a > 1);
kani::assume!(a > 2);
kani::assume!(a > 3);
kani::assume!(a > 4);
kani::assume!(a > 5);
kani::assume!(a > 6);
kani::assume!(a > 7);
"#
        .to_string())
    }

    fn name(&self) -> &str {
        "adversarial"
    }

    fn is_available(&self) -> bool {
        true
    }
}

struct CapturePromptProvider {
    last_prompt: Arc<Mutex<String>>,
}

#[async_trait]
impl LlmProvider for CapturePromptProvider {
    async fn complete(&self, prompt: &str, _opts: &CompletionOpts) -> Result<String> {
        *self.last_prompt.lock().expect("lock prompt capture") = prompt.to_string();
        Ok("kani::assume!(a > 0);".to_string())
    }

    fn name(&self) -> &str {
        "capture"
    }

    fn is_available(&self) -> bool {
        true
    }
}

fn sample_request() -> HarnessRequest {
    HarnessRequest {
        target_fn: FunctionSignature {
            name: "unchecked_add".to_string(),
            params: vec!["a: u64".to_string(), "b: u64".to_string()],
            return_type: "u64".to_string(),
        },
        source_context: "fn unchecked_add(a: u64, b: u64) -> u64 { a + b }".to_string(),
        graph_context: vec![],
        context_char_budget: 1_200,
        rule_trigger: RuleTrigger {
            rule_id: "CRYPTO-001".to_string(),
            reason: "unchecked arithmetic".to_string(),
        },
        required_assertion: AssertionSpec::NoOverflow {
            operation: "a + b".to_string(),
        },
        max_bound: 64,
    }
}

#[test]
fn parse_assume_lines_filters_and_caps_adversarial_output() {
    let parsed = parse_assume_lines(
        r#"
kani::assume!(x < 10);
kani::assume!(false);
assert!(x < 10);
kani::assume!(x % 2 == 0);
kani::assume!(0 == 1);
"#,
    );
    assert_eq!(
        parsed,
        vec![
            "kani::assume!(x < 10);".to_string(),
            "kani::assume!(x % 2 == 0);".to_string()
        ]
    );
}

#[test]
fn parse_assume_lines_caps_at_eight() {
    let mut raw = String::new();
    for idx in 0..12 {
        raw.push_str(&format!("kani::assume!(x > {});\n", idx));
    }
    let parsed = parse_assume_lines(&raw);
    assert_eq!(parsed.len(), 8);
}

#[tokio::test]
async fn build_without_llm_still_generates_runnable_harness() {
    let gate = Arc::new(EvidenceGate::without_sandbox_for_tests());
    let scaffolder = KaniHarnessScaffolder::without_sandbox_for_tests(None, gate);

    let result = scaffolder
        .build(&sample_request())
        .await
        .expect("build harness");

    assert!(result.harness_code.contains("pub fn harness()"));
    assert!(result.gate_level_reached >= 2);
    assert!(!result.llm_assume_hints_used);
}

#[tokio::test]
async fn build_sets_llm_assume_hints_used_when_hints_survive_filtering() {
    let gate = Arc::new(EvidenceGate::without_sandbox_for_tests());
    let provider = Arc::new(AdversarialHintProvider);
    let scaffolder = KaniHarnessScaffolder::without_sandbox_for_tests(Some(provider), gate);

    let result = scaffolder
        .build(&sample_request())
        .await
        .expect("build harness");
    assert!(result.llm_assume_hints_used);
    assert!(
        result.harness_code.contains("kani::assume"),
        "filtered hints should be included in harness code"
    );
    assert!(
        result.harness_code.contains("kani::assert(result >= a);"),
        "required assertion must be deterministic from rule trigger"
    );
    assert!(
        !result.harness_code.contains("println!(\"oops\")"),
        "non-assume LLM output must be dropped"
    );
}

#[test]
fn assertion_spec_display_never_uses_llm_data_path() {
    let call_site = CodeLocation {
        crate_name: "crypto".to_string(),
        module: "arith".to_string(),
        file: "src/lib.rs".into(),
        line_range: (10, 10),
        snippet: None,
    };
    let spec = AssertionSpec::NoUnwrapPanic { call_site };
    let rendered = spec.to_string();
    assert!(rendered.contains("NoUnwrapPanic"));
    assert!(!rendered.contains("llm"));

    let range = AssertionSpec::FieldElementInRange {
        var: "x".to_string(),
        max: BigUint::from(100u32),
    };
    assert!(range.to_string().contains("FieldElementInRange"));
}

#[tokio::test]
async fn graph_context_is_preferred_over_raw_source_context_when_present() {
    let gate = Arc::new(EvidenceGate::without_sandbox_for_tests());
    let captured_prompt = Arc::new(Mutex::new(String::new()));
    let provider = Arc::new(CapturePromptProvider {
        last_prompt: captured_prompt.clone(),
    });
    let scaffolder = KaniHarnessScaffolder::without_sandbox_for_tests(Some(provider), gate);

    let mut req = sample_request();
    req.source_context = "SOURCE_CONTEXT_SENTINEL".to_string();
    req.graph_context = vec![GraphContextEntry {
        node_id: "symbol:core::unchecked_add".to_string(),
        content: "GRAPH_CONTEXT_SENTINEL".to_string(),
    }];

    let _result = scaffolder.build(&req).await.expect("build harness");
    let prompt = captured_prompt.lock().expect("lock captured prompt").clone();
    assert!(
        prompt.contains("GRAPH_CONTEXT_SENTINEL"),
        "graph-backed context should be injected into the LLM prompt"
    );
    assert!(
        !prompt.contains("SOURCE_CONTEXT_SENTINEL"),
        "raw source_context should not be used when graph-backed context exists"
    );
}

#[tokio::test]
async fn source_context_fallback_is_used_when_graph_context_is_empty() {
    let gate = Arc::new(EvidenceGate::without_sandbox_for_tests());
    let captured_prompt = Arc::new(Mutex::new(String::new()));
    let provider = Arc::new(CapturePromptProvider {
        last_prompt: captured_prompt.clone(),
    });
    let scaffolder = KaniHarnessScaffolder::without_sandbox_for_tests(Some(provider), gate);

    let mut req = sample_request();
    req.source_context = "SOURCE_CONTEXT_FALLBACK_SENTINEL".to_string();
    req.graph_context.clear();

    let _result = scaffolder.build(&req).await.expect("build harness");
    let prompt = captured_prompt.lock().expect("lock captured prompt").clone();
    assert!(
        prompt.contains("SOURCE_CONTEXT_FALLBACK_SENTINEL"),
        "source_context should be used when no graph context entries are available"
    );
}

#[tokio::test]
async fn graph_context_budget_is_honored_in_prompt_context_section() {
    let gate = Arc::new(EvidenceGate::without_sandbox_for_tests());
    let captured_prompt = Arc::new(Mutex::new(String::new()));
    let provider = Arc::new(CapturePromptProvider {
        last_prompt: captured_prompt.clone(),
    });
    let scaffolder = KaniHarnessScaffolder::without_sandbox_for_tests(Some(provider), gate);

    let mut req = sample_request();
    req.graph_context = vec![GraphContextEntry {
        node_id: "symbol:big".to_string(),
        content: format!(
            "{}{}",
            "A".repeat(512),
            "TRUNCATION_TAIL_SENTINEL_SHOULD_NOT_APPEAR"
        ),
    }];
    req.context_char_budget = 64;

    let _result = scaffolder.build(&req).await.expect("build harness");
    let prompt = captured_prompt.lock().expect("lock captured prompt").clone();
    assert!(
        !prompt.contains("TRUNCATION_TAIL_SENTINEL_SHOULD_NOT_APPEAR"),
        "context packer should enforce budget and trim oversized graph context"
    );
}
