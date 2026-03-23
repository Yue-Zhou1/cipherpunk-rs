use llm_eval::{EvalResult, MarkdownReporter};

#[test]
fn markdown_report_marks_regressions_against_baseline() {
    let current = vec![
        EvalResult {
            fixture_id: "f1".to_string(),
            role: "Scaffolding".to_string(),
            passed: false,
            skipped: false,
            assertions_passed: 0,
            assertions_total: 1,
            duration_ms: 10,
            provider: "openai".to_string(),
            model: Some("gpt-4o-mini".to_string()),
            failure_reasons: vec!["boom".to_string()],
        },
        EvalResult {
            fixture_id: "f2".to_string(),
            role: "Scaffolding".to_string(),
            passed: true,
            skipped: false,
            assertions_passed: 1,
            assertions_total: 1,
            duration_ms: 10,
            provider: "openai".to_string(),
            model: Some("gpt-4o-mini".to_string()),
            failure_reasons: vec![],
        },
    ];

    let baseline = vec![
        EvalResult {
            fixture_id: "f1".to_string(),
            role: "Scaffolding".to_string(),
            passed: true,
            skipped: false,
            assertions_passed: 1,
            assertions_total: 1,
            duration_ms: 10,
            provider: "openai".to_string(),
            model: Some("gpt-4o-mini".to_string()),
            failure_reasons: vec![],
        },
        EvalResult {
            fixture_id: "f2".to_string(),
            role: "Scaffolding".to_string(),
            passed: false,
            skipped: false,
            assertions_passed: 0,
            assertions_total: 1,
            duration_ms: 10,
            provider: "openai".to_string(),
            model: Some("gpt-4o-mini".to_string()),
            failure_reasons: vec!["old".to_string()],
        },
    ];

    let markdown = MarkdownReporter::generate(&current, Some(&baseline));
    assert!(markdown.contains("REGRESSED"));
    assert!(markdown.contains("FIXED"));
}
