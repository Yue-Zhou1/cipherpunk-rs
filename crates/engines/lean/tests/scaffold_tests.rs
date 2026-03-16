use anyhow::Result;
use async_trait::async_trait;
use engine_lean::scaffold::generate_lean_stub;
use llm::{CompletionOpts, LlmProvider};

struct FixedProvider(String);

#[async_trait]
impl LlmProvider for FixedProvider {
    async fn complete(&self, _prompt: &str, _opts: &CompletionOpts) -> Result<String> {
        Ok(self.0.clone())
    }

    fn name(&self) -> &str {
        "fixed"
    }

    fn is_available(&self) -> bool {
        true
    }
}

#[tokio::test]
async fn generate_stub_returns_llm_output() {
    let provider =
        FixedProvider("import Mathlib\ntheorem foo_invariant : True := sorry".to_string());
    let stub = generate_lean_stub("foo", "fn foo(x: u64) -> u64 { x }", &provider)
        .await
        .unwrap();
    assert!(stub.contains("import Mathlib"));
    assert!(stub.contains("sorry"));
}

#[tokio::test]
async fn generate_stub_truncates_oversized_snippet() {
    let provider = FixedProvider("import Mathlib\ntheorem bar : True := sorry".to_string());
    let big = "x".repeat(10_000);
    let result = generate_lean_stub("bar", &big, &provider).await;
    assert!(result.is_ok());
}
