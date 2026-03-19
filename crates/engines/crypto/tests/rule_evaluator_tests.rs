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
            lines.push("let nonce = 0u64;".to_string());
        } else if i == 12 {
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

#[tokio::test]
async fn semantic_checks_are_executed_and_gate_matches() {
    let dir = tempdir().expect("tempdir");
    fs::write(
        dir.path().join("CRYPTO-TST-001.yaml"),
        r#"id: CRYPTO-TST-001
title: "nonce safety check"
severity: High
category: CryptoMisuse
description: "test"
detection:
  patterns:
    - type: function_call
      name_matches: ["aead_encrypt"]
  semantic_checks:
    - nonce_is_not_bound_to_session_id
references: []
remediation: "test"
"#,
    )
    .expect("write rule");
    let evaluator = RuleEvaluator::load_from_dir(dir.path()).expect("load evaluator");

    let nonce_safe = SourceFile {
        crate_name: "fixture-crate".to_string(),
        path: fixture_dir().join("nonce_safe.rs"),
        module: "nonce_safe".to_string(),
        content: r#"
pub fn safe_path(session_id: u64, key: [u8; 32], msg: &[u8]) {
    let nonce = session_id;
    aead_encrypt(key, nonce, msg);
}

fn aead_encrypt(_key: [u8; 32], _nonce: u64, _msg: &[u8]) {}
"#
        .to_string(),
    };
    assert!(
        evaluator.evaluate_file(&nonce_safe).await.is_empty(),
        "semantic check should block match when nonce is session-bound"
    );

    let nonce_unsafe = SourceFile {
        crate_name: "fixture-crate".to_string(),
        path: fixture_dir().join("nonce_unsafe.rs"),
        module: "nonce_unsafe".to_string(),
        content: r#"
pub fn unsafe_path(key: [u8; 32], msg: &[u8]) {
    let nonce = 0u64;
    aead_encrypt(key, nonce, msg);
}

fn aead_encrypt(_key: [u8; 32], _nonce: u64, _msg: &[u8]) {}
"#
        .to_string(),
    };
    let matches = evaluator.evaluate_file(&nonce_unsafe).await;
    assert!(
        matches.iter().any(|m| m.rule_id == "CRYPTO-TST-001"),
        "semantic check should emit finding for suspicious nonce initialization"
    );
}

#[tokio::test]
async fn evaluator_supports_method_macro_and_path_patterns() {
    let dir = tempdir().expect("tempdir");
    fs::write(
        dir.path().join("CRYPTO-TST-201.yaml"),
        r#"id: CRYPTO-TST-201
title: "method call match"
severity: Medium
category: CryptoMisuse
description: "test"
detection:
  patterns:
    - type: method_call
      name_matches: ["unwrap_or_default"]
references: []
remediation: "test"
"#,
    )
    .expect("write rule");
    fs::write(
        dir.path().join("CRYPTO-TST-202.yaml"),
        r#"id: CRYPTO-TST-202
title: "macro call match"
severity: Medium
category: CryptoMisuse
description: "test"
detection:
  patterns:
    - type: macro_call
      name_matches: ["danger_macro"]
references: []
remediation: "test"
"#,
    )
    .expect("write rule");
    fs::write(
        dir.path().join("CRYPTO-TST-203.yaml"),
        r#"id: CRYPTO-TST-203
title: "path contains match"
severity: Low
category: CryptoMisuse
description: "test"
detection:
  patterns:
    - type: path_contains
      name_matches: ["target_path_hit"]
references: []
remediation: "test"
"#,
    )
    .expect("write rule");

    let evaluator = RuleEvaluator::load_from_dir(dir.path()).expect("load evaluator");
    let file = SourceFile {
        crate_name: "fixture-crate".to_string(),
        path: fixture_dir().join("target_path_hit.rs"),
        module: "target_path_hit".to_string(),
        content: r#"
macro_rules! danger_macro {
    () => { 1 };
}

pub fn trigger() {
    let res = Result::<u64, &'static str>::Ok(1);
    let _ = res.unwrap_or_default();
    let _ = danger_macro!();
}
"#
        .to_string(),
    };

    let matches = evaluator.evaluate_file(&file).await;
    let ids = matches
        .iter()
        .map(|m| m.rule_id.as_str())
        .collect::<HashSet<_>>();
    assert!(ids.contains("CRYPTO-TST-201"));
    assert!(ids.contains("CRYPTO-TST-202"));
    assert!(ids.contains("CRYPTO-TST-203"));
}

#[test]
fn load_from_dir_rejects_unknown_pattern_and_semantic_check_ids() {
    let unknown_pattern = tempdir().expect("tempdir");
    fs::write(
        unknown_pattern.path().join("bad-pattern.yaml"),
        r#"id: CRYPTO-TST-301
title: "bad pattern"
severity: Low
category: CryptoMisuse
description: "test"
detection:
  patterns:
    - type: unsupported_pattern
      name_matches: ["x"]
references: []
remediation: "test"
"#,
    )
    .expect("write bad pattern rule");
    let err = match RuleEvaluator::load_from_dir(unknown_pattern.path()) {
        Ok(_) => panic!("load_from_dir should fail for unsupported pattern type"),
        Err(err) => err,
    };
    assert!(
        err.to_string().contains("unsupported pattern type"),
        "unexpected error: {err:#}"
    );

    let unknown_semantic = tempdir().expect("tempdir");
    fs::write(
        unknown_semantic.path().join("bad-semantic.yaml"),
        r#"id: CRYPTO-TST-302
title: "bad semantic check"
severity: Low
category: CryptoMisuse
description: "test"
detection:
  patterns:
    - type: function_call
      name_matches: ["x"]
  semantic_checks:
    - unknown_check_id
references: []
remediation: "test"
"#,
    )
    .expect("write bad semantic rule");
    let err = match RuleEvaluator::load_from_dir(unknown_semantic.path()) {
        Ok(_) => panic!("load_from_dir should fail for unsupported semantic check"),
        Err(err) => err,
    };
    assert!(
        err.to_string().contains("unsupported semantic check"),
        "unexpected error: {err:#}"
    );
}

#[tokio::test]
async fn rule_matches_include_ir_node_ids_for_resolvable_symbol_matches() {
    let evaluator = RuleEvaluator::load_from_dir(&rule_dir()).expect("load evaluator");
    let path = fixture_dir().join("crypto001_nonce_reuse.rs");
    let file = source_file(&path);

    let matches = evaluator.evaluate_file(&file).await;
    let target = matches
        .iter()
        .find(|matched| matched.rule_id == "CRYPTO-001")
        .expect("CRYPTO-001 match");

    assert!(
        target
            .ir_node_ids
            .iter()
            .any(|id| id == &format!("file:{}", path.display())),
        "rule match should include file-level provenance id"
    );
    assert!(
        target
            .ir_node_ids
            .iter()
            .any(|id| id.contains("symbol:") && id.contains("aead_encrypt")),
        "rule match should include symbol-level provenance id when function name is resolvable"
    );
}

#[tokio::test]
async fn pattern_only_rules_emit_all_pattern_matches_not_just_first() {
    let dir = tempdir().expect("tempdir");
    fs::write(
        dir.path().join("CRYPTO-TST-401.yaml"),
        r#"id: CRYPTO-TST-401
title: "multi-pattern rule"
severity: Medium
category: CryptoMisuse
description: "test"
detection:
  patterns:
    - type: function_call
      name_matches: ["aead_encrypt"]
    - type: macro_call
      name_matches: ["danger_macro"]
references: []
remediation: "test"
"#,
    )
    .expect("write rule");
    let evaluator = RuleEvaluator::load_from_dir(dir.path()).expect("load evaluator");

    let mut lines = Vec::<String>::new();
    lines.push("macro_rules! danger_macro { () => { 1 } }".to_string());
    lines.push("fn run() { aead_encrypt([1u8; 32], 0u64, b\"x\"); }".to_string());
    for idx in 0..30 {
        lines.push(format!("let pad_{idx} = {idx};"));
    }
    lines.push("fn later() { let _ = danger_macro!(); }".to_string());

    let file = SourceFile {
        crate_name: "fixture-crate".to_string(),
        path: fixture_dir().join("multi_pattern.rs"),
        module: "multi_pattern".to_string(),
        content: lines.join("\n"),
    };

    let matches = evaluator.evaluate_file(&file).await;
    let rule_matches = matches
        .into_iter()
        .filter(|m| m.rule_id == "CRYPTO-TST-401")
        .collect::<Vec<_>>();

    assert_eq!(
        rule_matches.len(),
        2,
        "pattern-only rule should emit one match per matching pattern hit"
    );
}

#[tokio::test]
async fn semantic_checks_are_documented_and_behavior_is_and() {
    let dir = tempdir().expect("tempdir");
    fs::write(
        dir.path().join("CRYPTO-TST-402.yaml"),
        r#"id: CRYPTO-TST-402
title: "and semantics check"
severity: Medium
category: CryptoMisuse
description: "test"
detection:
  patterns:
    - type: function_call
      name_matches: ["aead_encrypt"]
  semantic_checks:
    - nonce_is_not_bound_to_session_id
    - hardcoded_secret_present
references: []
remediation: "test"
"#,
    )
    .expect("write rule");
    let evaluator = RuleEvaluator::load_from_dir(dir.path()).expect("load evaluator");

    let only_nonce = SourceFile {
        crate_name: "fixture-crate".to_string(),
        path: fixture_dir().join("only_nonce.rs"),
        module: "only_nonce".to_string(),
        content: r#"
fn run() {
    let nonce = 0u64;
    aead_encrypt([1u8; 32], nonce, b"x");
}
fn aead_encrypt(_key: [u8; 32], _nonce: u64, _msg: &[u8]) {}
"#
        .to_string(),
    };
    assert!(
        evaluator.evaluate_file(&only_nonce).await.is_empty(),
        "multiple semantic checks should require all checks to match (AND)"
    );

    let both = SourceFile {
        crate_name: "fixture-crate".to_string(),
        path: fixture_dir().join("nonce_and_secret.rs"),
        module: "nonce_and_secret".to_string(),
        content: r#"
fn run() {
    let nonce = 0u64;
    let key = [9u8; 32];
    aead_encrypt(key, nonce, b"x");
}
fn aead_encrypt(_key: [u8; 32], _nonce: u64, _msg: &[u8]) {}
"#
        .to_string(),
    };
    let matches = evaluator.evaluate_file(&both).await;
    assert!(
        matches.iter().any(|m| m.rule_id == "CRYPTO-TST-402"),
        "rule should match only when all semantic checks are satisfied"
    );
}

#[tokio::test]
async fn semantic_check_only_rule_is_supported() {
    let dir = tempdir().expect("tempdir");
    fs::write(
        dir.path().join("CRYPTO-TST-403.yaml"),
        r#"id: CRYPTO-TST-403
title: "semantic-only rule"
severity: Medium
category: CryptoMisuse
description: "test"
detection:
  patterns: []
  semantic_checks:
    - hardcoded_secret_present
references: []
remediation: "test"
"#,
    )
    .expect("write rule");
    let evaluator = RuleEvaluator::load_from_dir(dir.path()).expect("load evaluator");

    let file = SourceFile {
        crate_name: "fixture-crate".to_string(),
        path: fixture_dir().join("semantic_only.rs"),
        module: "semantic_only".to_string(),
        content: r#"
fn run() {
    let key = [7u8; 32];
    let _ = key;
}
"#
        .to_string(),
    };

    let matches = evaluator.evaluate_file(&file).await;
    assert!(
        matches.iter().any(|m| m.rule_id == "CRYPTO-TST-403"),
        "semantic-only rules should emit matches when checks trigger"
    );
}
