use std::path::PathBuf;

#[test]
fn forbid_direct_llm_complete_calls_outside_llm_call() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("services/")
        .parent()
        .expect("crates/")
        .parent()
        .expect("repo root")
        .to_path_buf();

    let mut violations = Vec::<String>::new();
    for entry in walkdir::WalkDir::new(root.join("crates"))
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        if entry.path().extension().and_then(|v| v.to_str()) != Some("rs") {
            continue;
        }
        let path = entry.path();
        let text = std::fs::read_to_string(path).expect("read source");
        for (idx, line) in text.lines().enumerate() {
            if !line.contains(".complete(") {
                continue;
            }
            let normalized = path.to_string_lossy().replace('\\', "/");
            let is_allowed = normalized.ends_with("/crates/services/llm/src/provider.rs")
                || normalized.ends_with("/crates/services/llm/tests/provider_tests.rs")
                || normalized.ends_with("/crates/services/llm/tests/direct_complete_lint_tests.rs");
            if !is_allowed {
                violations.push(format!("{}:{}", path.display(), idx + 1));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "direct provider.complete() call(s) found:\n{}",
        violations.join("\n")
    );
}
