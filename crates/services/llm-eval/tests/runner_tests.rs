use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use llm::{CompletionOpts, LlmProvider, TemplateFallback};
use llm_eval::{
    EvalAssertion, EvalFixture, EvalRunner, TemplateFallbackSupport, load_fixtures_from_dir,
};

struct StaticProvider {
    response: String,
}

#[async_trait]
impl LlmProvider for StaticProvider {
    async fn complete(&self, _prompt: &str, _opts: &CompletionOpts) -> Result<String> {
        Ok(self.response.clone())
    }

    fn name(&self) -> &str {
        "static"
    }

    fn is_available(&self) -> bool {
        true
    }
}

#[tokio::test]
async fn runner_evaluates_assertions() {
    let runner = EvalRunner::new(Arc::new(StaticProvider {
        response: r#"{"domains":[{"id":"crypto"}]}"#.to_string(),
    }));

    let fixture = EvalFixture {
        id: "fixture-1".to_string(),
        role: "SearchHints".to_string(),
        prompt: "ignored".to_string(),
        template_fallback: TemplateFallbackSupport::Supported,
        assertions: vec![
            EvalAssertion::JsonValid,
            EvalAssertion::FieldNotEmpty {
                path: "/domains".to_string(),
            },
        ],
    };

    let result = runner.run_fixture(&fixture).await;
    assert!(result.passed);
    assert_eq!(result.assertions_passed, 2);
    assert!(!result.skipped);
}

#[tokio::test]
async fn template_fallback_marks_skip_for_unsupported_fixture() {
    let runner = EvalRunner::new(Arc::new(TemplateFallback));

    let fixture = EvalFixture {
        id: "skip-template".to_string(),
        role: "SearchHints".to_string(),
        prompt: "Return JSON".to_string(),
        template_fallback: TemplateFallbackSupport::Skip,
        assertions: vec![EvalAssertion::JsonValid],
    };

    let result = runner.run_fixture(&fixture).await;
    assert!(result.passed);
    assert!(result.skipped);
}

#[tokio::test]
async fn built_in_fixtures_work_with_template_fallback_support_rules() {
    let fixtures_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures");
    let fixtures = load_fixtures_from_dir(&fixtures_dir).expect("load built-in fixtures");
    assert!(
        fixtures.len() >= 9,
        "expected at least 9 built-in fixtures, got {}",
        fixtures.len()
    );

    let runner = EvalRunner::new(Arc::new(TemplateFallback));
    let results = runner.run_all(&fixtures).await;

    let required_results: Vec<_> = fixtures
        .iter()
        .zip(results.iter())
        .filter(|(fixture, _)| fixture.template_fallback == TemplateFallbackSupport::Required)
        .map(|(_, result)| result)
        .collect();
    assert!(
        required_results
            .iter()
            .all(|result| result.passed && !result.skipped),
        "required template-fallback fixtures must pass"
    );

    let skipped_results: Vec<_> = fixtures
        .iter()
        .zip(results.iter())
        .filter(|(fixture, _)| fixture.template_fallback == TemplateFallbackSupport::Skip)
        .map(|(_, result)| result)
        .collect();
    assert!(
        skipped_results.iter().all(|result| result.skipped),
        "skip template-fallback fixtures must be reported as skipped"
    );
}
