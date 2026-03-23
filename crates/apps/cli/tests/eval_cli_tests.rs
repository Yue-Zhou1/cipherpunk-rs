use std::fs;
use std::path::Path;

use audit_agent_cli::{EvalArgs, run_eval};
use llm_eval::EvalResult;
use tempfile::tempdir;

fn write_file(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create parent");
    }
    std::fs::write(path, content).expect("write file");
}

#[tokio::test]
async fn eval_command_saves_baseline_results() {
    let temp = tempdir().expect("tempdir");
    let fixtures_dir = temp.path().join("fixtures");
    let baseline_path = temp.path().join("baseline.json");

    write_file(
        &fixtures_dir.join("fixtures.yaml"),
        r#"
- id: fixture-pass
  role: Scaffolding
  prompt: "generate harness for field_mul"
  template_fallback: required
  assertions:
    - type: ContainsKeyword
      value: "fn harness"
"#,
    );

    run_eval(EvalArgs {
        provider: Some("template".to_string()),
        baseline: Some(baseline_path.clone()),
        compare: None,
        fixtures: Some(fixtures_dir),
    })
    .await
    .expect("eval should pass");

    let data = fs::read(&baseline_path).expect("read baseline");
    let results: Vec<EvalResult> = serde_json::from_slice(&data).expect("parse baseline json");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].fixture_id, "fixture-pass");
    assert!(results[0].passed);
}

#[tokio::test]
async fn eval_command_fails_on_regression_against_baseline() {
    let temp = tempdir().expect("tempdir");
    let fixtures_dir = temp.path().join("fixtures");
    let baseline_path = temp.path().join("baseline.json");

    write_file(
        &fixtures_dir.join("fixtures.yaml"),
        r#"
- id: fixture-regress
  role: Scaffolding
  prompt: "generate harness for field_mul"
  template_fallback: required
  assertions:
    - type: ContainsKeyword
      value: "does-not-exist"
"#,
    );

    let baseline = vec![EvalResult {
        fixture_id: "fixture-regress".to_string(),
        role: "Scaffolding".to_string(),
        passed: true,
        skipped: false,
        assertions_passed: 1,
        assertions_total: 1,
        duration_ms: 1,
        provider: "template-fallback".to_string(),
        model: None,
        failure_reasons: vec![],
    }];
    write_file(
        &baseline_path,
        &serde_json::to_string_pretty(&baseline).expect("serialize baseline"),
    );

    let err = run_eval(EvalArgs {
        provider: Some("template".to_string()),
        baseline: None,
        compare: Some(baseline_path),
        fixtures: Some(fixtures_dir),
    })
    .await
    .expect_err("eval should fail due to regression");

    assert!(err.to_string().contains("regression"));
}
