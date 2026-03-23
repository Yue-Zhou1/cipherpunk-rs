use crate::runner::EvalResult;

pub struct MarkdownReporter;

impl MarkdownReporter {
    pub fn generate(results: &[EvalResult], baseline: Option<&[EvalResult]>) -> String {
        let mut markdown = String::new();
        markdown.push_str("# LLM Evaluation Report\n\n");

        let provider = results
            .first()
            .map(|r| r.provider.as_str())
            .unwrap_or("unknown");
        let model = results
            .first()
            .and_then(|r| r.model.as_deref())
            .unwrap_or("unknown");
        markdown.push_str(&format!("**Provider:** {provider}\n"));
        markdown.push_str(&format!("**Model:** {model}\n\n"));

        let total = results.len();
        let skipped = results.iter().filter(|r| r.skipped).count();
        let passed = results.iter().filter(|r| r.passed && !r.skipped).count();
        let failed = results.iter().filter(|r| !r.passed && !r.skipped).count();
        markdown.push_str(&format!(
            "**Results:** {passed} passed, {failed} failed, {skipped} skipped ({total} total)\n\n"
        ));

        markdown.push_str("| Fixture | Role | Status | Assertions | Duration | Regressions |\n");
        markdown.push_str("|---------|------|--------|------------|----------|-------------|\n");
        for result in results {
            let status = if result.skipped {
                "SKIP"
            } else if result.passed {
                "PASS"
            } else {
                "FAIL"
            };
            let assertions = format!("{}/{}", result.assertions_passed, result.assertions_total);
            let duration = format!("{}ms", result.duration_ms);
            let regression = baseline
                .map(|b| regression_status(result, b))
                .unwrap_or("-");
            markdown.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} |\n",
                result.fixture_id, result.role, status, assertions, duration, regression
            ));
        }

        let failures: Vec<_> = results.iter().filter(|r| !r.passed && !r.skipped).collect();
        if !failures.is_empty() {
            markdown.push_str("\n## Failures\n\n");
            for result in failures {
                markdown.push_str(&format!("### {}\n\n", result.fixture_id));
                for reason in &result.failure_reasons {
                    markdown.push_str(&format!("- {reason}\n"));
                }
                markdown.push('\n');
            }
        }

        markdown
    }
}

fn regression_status<'a>(result: &EvalResult, baseline: &'a [EvalResult]) -> &'a str {
    let Some(old) = baseline
        .iter()
        .find(|old| old.fixture_id == result.fixture_id)
    else {
        return "NEW";
    };

    if old.skipped == result.skipped && old.passed == result.passed {
        return "-";
    }
    if old.passed && !result.passed && !result.skipped {
        return "REGRESSED";
    }
    if !old.passed && result.passed && !result.skipped {
        return "FIXED";
    }
    if old.skipped && !result.skipped {
        return "UNSKIPPED";
    }
    if !old.skipped && result.skipped {
        return "NOW-SKIPPED";
    }
    "-"
}
