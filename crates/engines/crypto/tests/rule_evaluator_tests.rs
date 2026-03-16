use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use engine_crypto::rules::{RuleEvaluator, SourceFile};
use tempfile::tempdir;

fn repo_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .expect("engines/")
        .parent()
        .expect("crates/")
        .parent()
        .expect("repo root")
        .to_path_buf()
}

fn rule_dir() -> PathBuf {
    repo_root().join("rules/crypto-misuse")
}

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rust-crypto")
}

fn source_file(path: &Path) -> SourceFile {
    SourceFile {
        crate_name: "fixture-crate".to_string(),
        path: path.to_path_buf(),
        module: path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("fixture")
            .to_string(),
        content: fs::read_to_string(path).expect("read fixture"),
    }
}

#[test]
fn loads_rules_from_yaml_at_startup() {
    let evaluator = RuleEvaluator::load_from_dir(&rule_dir()).expect("load rule evaluator");
    assert_eq!(evaluator.rules().len(), 8);
    assert_eq!(evaluator.rules()[0].id, "CRYPTO-001");
    assert_eq!(evaluator.rules()[7].id, "CRYPTO-008");
}

#[tokio::test]
async fn all_phase1_rules_fire_on_synthetic_fixtures() {
    let evaluator = RuleEvaluator::load_from_dir(&rule_dir()).expect("load evaluator");
    let mut matched = HashSet::<String>::new();

    for entry in fs::read_dir(fixture_dir()).expect("fixture dir exists") {
        let entry = entry.expect("fixture entry");
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
            continue;
        }
        let file = source_file(&path);
        for rule_match in evaluator.evaluate_file(&file).await {
            matched.insert(rule_match.rule_id);
        }
    }

    for id in [
        "CRYPTO-001",
        "CRYPTO-002",
        "CRYPTO-003",
        "CRYPTO-004",
        "CRYPTO-005",
        "CRYPTO-006",
        "CRYPTO-007",
        "CRYPTO-008",
    ] {
        assert!(matched.contains(id), "missing match for {id}");
    }
}

#[tokio::test]
async fn each_match_includes_exact_file_line_range_and_ten_line_snippet() {
    let evaluator = RuleEvaluator::load_from_dir(&rule_dir()).expect("load evaluator");
    let mut lines = Vec::new();
    for i in 1..=20 {
        if i == 11 {
            lines.push("aead_encrypt(key, nonce, msg);".to_string());
        } else {
            lines.push(format!("let line_{i} = {i};"));
        }
    }
    let content = lines.join("\n");
    let synthetic = SourceFile {
        crate_name: "fixture-crate".to_string(),
        path: fixture_dir().join("synthetic_10_line_snippet.rs"),
        module: "synthetic".to_string(),
        content,
    };

    let matches = evaluator.evaluate_file(&synthetic).await;
    let target = matches
        .iter()
        .find(|m| m.rule_id == "CRYPTO-001")
        .expect("CRYPTO-001 match");

    assert_eq!(target.location.file, synthetic.path);
    let snippet = target
        .location
        .snippet
        .as_ref()
        .expect("snippet should be present");
    let snippet_line_count = snippet.lines().count() as u32;
    assert_eq!(snippet_line_count, 10);
    assert_eq!(
        target.location.line_range.1 - target.location.line_range.0 + 1,
        10
    );
}

#[tokio::test]
async fn false_positive_rate_below_twenty_percent_on_twenty_clean_files() {
    let evaluator = RuleEvaluator::load_from_dir(&rule_dir()).expect("load evaluator");
    let dir = tempdir().expect("tempdir");

    let mut flagged_files = 0u32;
    for idx in 0..20u32 {
        let path = dir.path().join(format!("clean_{idx}.rs"));
        fs::write(
            &path,
            format!(
                "pub fn clean_{idx}() {{\n  let digest = idx_{idx} + 1;\n  let _ = digest;\n}}\n"
            ),
        )
        .expect("write clean fixture");
        let file = source_file(&path);
        if !evaluator.evaluate_file(&file).await.is_empty() {
            flagged_files += 1;
        }
    }

    let fp_rate = (flagged_files as f64 / 20.0) * 100.0;
    assert!(
        fp_rate < 20.0,
        "false positive rate {fp_rate}% must be below 20%"
    );
}
