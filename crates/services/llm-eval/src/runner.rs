use std::sync::Arc;
use std::time::Instant;

use llm::{
    ArchitectureOverview, CandidateDraft, ChecklistPlan, CompletionOpts, LlmProvider, LlmRole,
    llm_call_traced,
};
use serde::{Deserialize, Serialize};

use crate::fixture::{EvalAssertion, EvalFixture, TemplateFallbackSupport};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalResult {
    pub fixture_id: String,
    pub role: String,
    pub passed: bool,
    #[serde(default)]
    pub skipped: bool,
    pub assertions_passed: usize,
    pub assertions_total: usize,
    pub duration_ms: u64,
    pub provider: String,
    pub model: Option<String>,
    pub failure_reasons: Vec<String>,
}

pub struct EvalRunner {
    provider: Arc<dyn LlmProvider>,
}

impl EvalRunner {
    pub fn new(provider: Arc<dyn LlmProvider>) -> Self {
        Self { provider }
    }

    pub async fn run_fixture(&self, fixture: &EvalFixture) -> EvalResult {
        let role = parse_role(&fixture.role);
        let start = Instant::now();

        if is_template_provider(self.provider.name())
            && fixture.template_fallback == TemplateFallbackSupport::Skip
        {
            return skipped_result(
                fixture,
                start.elapsed().as_millis() as u64,
                "template-fallback",
            );
        }

        let response = match llm_call_traced(
            self.provider.as_ref(),
            role,
            &fixture.prompt,
            &CompletionOpts::default(),
        )
        .await
        {
            Ok((text, provenance)) => {
                if is_template_provider(&provenance.provider)
                    && fixture.template_fallback == TemplateFallbackSupport::Skip
                {
                    return skipped_result(
                        fixture,
                        start.elapsed().as_millis() as u64,
                        provenance.provider.as_str(),
                    );
                }
                (text, provenance.provider, provenance.model)
            }
            Err(err) => {
                return EvalResult {
                    fixture_id: fixture.id.clone(),
                    role: fixture.role.clone(),
                    passed: false,
                    skipped: false,
                    assertions_passed: 0,
                    assertions_total: fixture.assertions.len(),
                    duration_ms: start.elapsed().as_millis() as u64,
                    provider: self.provider.name().to_string(),
                    model: self.provider.model().map(ToOwned::to_owned),
                    failure_reasons: vec![format!("LLM call failed: {err}")],
                };
            }
        };

        let (response_text, provider_name, provider_model) = response;
        let mut passed_count = 0usize;
        let mut failures = Vec::new();

        for assertion in &fixture.assertions {
            match check_assertion(assertion, &response_text) {
                Ok(()) => passed_count += 1,
                Err(reason) => failures.push(reason),
            }
        }

        EvalResult {
            fixture_id: fixture.id.clone(),
            role: fixture.role.clone(),
            passed: failures.is_empty(),
            skipped: false,
            assertions_passed: passed_count,
            assertions_total: fixture.assertions.len(),
            duration_ms: start.elapsed().as_millis() as u64,
            provider: provider_name,
            model: provider_model,
            failure_reasons: failures,
        }
    }

    pub async fn run_all(&self, fixtures: &[EvalFixture]) -> Vec<EvalResult> {
        let mut results = Vec::with_capacity(fixtures.len());
        for fixture in fixtures {
            results.push(self.run_fixture(fixture).await);
        }
        results
    }
}

fn skipped_result(fixture: &EvalFixture, duration_ms: u64, provider: &str) -> EvalResult {
    EvalResult {
        fixture_id: fixture.id.clone(),
        role: fixture.role.clone(),
        passed: true,
        skipped: true,
        assertions_passed: 0,
        assertions_total: fixture.assertions.len(),
        duration_ms,
        provider: provider.to_string(),
        model: None,
        failure_reasons: vec![],
    }
}

fn is_template_provider(provider: &str) -> bool {
    provider.eq_ignore_ascii_case("template") || provider.eq_ignore_ascii_case("template-fallback")
}

fn check_assertion(assertion: &EvalAssertion, response: &str) -> Result<(), String> {
    match assertion {
        EvalAssertion::JsonValid => serde_json::from_str::<serde_json::Value>(response.trim())
            .map(|_| ())
            .map_err(|e| format!("JsonValid failed: {e}")),
        EvalAssertion::ContainsKeyword { value } => {
            if response
                .to_ascii_lowercase()
                .contains(&value.to_ascii_lowercase())
            {
                Ok(())
            } else {
                Err(format!("ContainsKeyword '{value}' not found"))
            }
        }
        EvalAssertion::NotContainsKeyword { value } => {
            if response
                .to_ascii_lowercase()
                .contains(&value.to_ascii_lowercase())
            {
                Err(format!("NotContainsKeyword '{value}' was found"))
            } else {
                Ok(())
            }
        }
        EvalAssertion::ParsesAsContract { contract } => {
            let parsed = match contract.as_str() {
                "ChecklistPlan" => serde_json::from_str::<ChecklistPlan>(response.trim())
                    .map(|_| ())
                    .map_err(anyhow::Error::from),
                "ArchitectureOverview" => {
                    serde_json::from_str::<ArchitectureOverview>(response.trim())
                        .map(|_| ())
                        .map_err(anyhow::Error::from)
                }
                "CandidateDraft" => serde_json::from_str::<CandidateDraft>(response.trim())
                    .map(|_| ())
                    .map_err(anyhow::Error::from),
                _ => serde_json::from_str::<serde_json::Value>(response)
                    .map(|_| ())
                    .map_err(anyhow::Error::from),
            };
            parsed.map_err(|e| format!("ParsesAsContract '{contract}' failed: {e}"))
        }
        EvalAssertion::FieldNotEmpty { path } => {
            let value: serde_json::Value = serde_json::from_str(response.trim())
                .map_err(|e| format!("FieldNotEmpty '{path}' — JSON parse failed: {e}"))?;
            let field = value
                .pointer(path)
                .ok_or_else(|| format!("FieldNotEmpty '{path}' — field not found"))?;
            match field {
                serde_json::Value::String(text) if text.trim().is_empty() => {
                    Err(format!("FieldNotEmpty '{path}' — field is empty string"))
                }
                serde_json::Value::Array(items) if items.is_empty() => {
                    Err(format!("FieldNotEmpty '{path}' — field is empty array"))
                }
                serde_json::Value::Null => Err(format!("FieldNotEmpty '{path}' — field is null")),
                _ => Ok(()),
            }
        }
        EvalAssertion::MaxChars { value } => {
            if response.len() <= *value {
                Ok(())
            } else {
                Err(format!("MaxChars {value} exceeded: got {}", response.len()))
            }
        }
        EvalAssertion::MinChars { value } => {
            if response.len() >= *value {
                Ok(())
            } else {
                Err(format!("MinChars {value} not met: got {}", response.len()))
            }
        }
    }
}

fn parse_role(role: &str) -> LlmRole {
    match role.to_ascii_lowercase().as_str() {
        "scaffolding" => LlmRole::Scaffolding,
        "searchhints" | "search_hints" => LlmRole::SearchHints,
        "proserendering" | "prose_rendering" => LlmRole::ProseRendering,
        "leanscaffold" | "lean_scaffold" => LlmRole::LeanScaffold,
        _ => LlmRole::Scaffolding,
    }
}

pub fn has_regressions(current: &[EvalResult], baseline: &[EvalResult]) -> bool {
    current.iter().any(|result| {
        baseline
            .iter()
            .find(|existing| existing.fixture_id == result.fixture_id)
            .map(|existing| existing.passed && !result.passed && !result.skipped)
            .unwrap_or(false)
    })
}

pub fn has_failures(results: &[EvalResult]) -> bool {
    results
        .iter()
        .any(|result| !result.passed && !result.skipped)
}
