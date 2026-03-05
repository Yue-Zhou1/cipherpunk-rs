use std::collections::HashSet;
use std::fs;
use std::path::Path;

use audit_agent_core::audit_config::BudgetConfig;
use audit_agent_core::finding::{Severity, VerificationStatus};
use engine_crypto::semantic::ra_client::SemanticIndex;
use engine_distributed::economic::{EconCategory, EconomicAttackChecker};
use intake::workspace::WorkspaceAnalyzer;
use tempfile::tempdir;

fn phase5_budget() -> BudgetConfig {
    BudgetConfig {
        kani_timeout_secs: 300,
        z3_timeout_secs: 600,
        fuzz_duration_secs: 3600,
        madsim_ticks: 100_000,
        max_llm_retries: 3,
        semantic_index_timeout_secs: 5,
    }
}

fn economic_rules_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates/")
        .parent()
        .expect("repo root")
        .join("rules/economic")
}

fn write_file(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent directory");
    }
    fs::write(path, content).expect("write file");
}

fn write_min_workspace(path: &Path, lib_rs: &str) {
    write_file(
        &path.join("Cargo.toml"),
        r#"[workspace]
members = ["rollup-core"]
resolver = "2"
"#,
    );
    write_file(
        &path.join("rollup-core/Cargo.toml"),
        r#"[package]
name = "rollup-core"
version = "0.1.0"
edition = "2024"
"#,
    );
    write_file(&path.join("rollup-core/src/lib.rs"), lib_rs);
}

#[test]
fn economic_checklist_has_minimum_vectors_across_required_categories() {
    let checker = EconomicAttackChecker::load_from_dir(&economic_rules_dir(), None)
        .expect("load economic checklist");

    assert!(
        checker.vectors().len() >= 6,
        "phase 5 requires at least 6 economic vectors"
    );

    let categories = checker
        .vectors()
        .iter()
        .map(|vector| vector.category.clone())
        .collect::<HashSet<_>>();
    assert!(categories.contains(&EconCategory::Sequencer));
    assert!(categories.contains(&EconCategory::Prover));
    assert!(categories.contains(&EconCategory::Sybil));
}

#[tokio::test]
async fn economic_findings_are_observation_unverified_and_use_default_description_without_llm() {
    let dir = tempdir().expect("tempdir");
    write_min_workspace(
        dir.path(),
        r#"
pub fn process_batch() {
    let txs = vec![1, 2, 3];
    let _ = txs.len();
}
"#,
    );

    let workspace = WorkspaceAnalyzer::analyze(dir.path()).expect("workspace");
    let semantic = SemanticIndex::build(&workspace, &phase5_budget())
        .await
        .expect("semantic index");
    let checker =
        EconomicAttackChecker::load_from_dir(&economic_rules_dir(), None).expect("load checklist");
    let findings = checker.analyze(&workspace, &semantic).await;

    assert!(!findings.is_empty(), "expected economic findings");
    for finding in &findings {
        assert_eq!(finding.severity, Severity::Observation);
        match &finding.verification_status {
            VerificationStatus::Unverified { reason } => {
                assert!(reason.contains("manual protocol review"));
            }
            other => panic!("expected unverified status, got {other:?}"),
        }
    }
    assert!(
        findings
            .iter()
            .any(|f| f.id.to_string() == "ECON-001" && f.impact.contains("ordering policy")),
        "expected ECON-001 fallback description from checklist"
    );
}

#[tokio::test]
async fn call_site_present_vector_triggers_when_pattern_exists() {
    let dir = tempdir().expect("tempdir");
    write_min_workspace(
        dir.path(),
        r#"
mod unsafe_batch_pricing {
    pub fn disable_fee_floor() {}
}

pub fn run() {
    unsafe_batch_pricing::disable_fee_floor();
}
"#,
    );

    let workspace = WorkspaceAnalyzer::analyze(dir.path()).expect("workspace");
    let semantic = SemanticIndex::build(&workspace, &phase5_budget())
        .await
        .expect("semantic index");
    let checker =
        EconomicAttackChecker::load_from_dir(&economic_rules_dir(), None).expect("load checklist");
    let findings = checker.analyze(&workspace, &semantic).await;

    assert!(
        findings.iter().any(|f| f.id.to_string() == "ECON-006"),
        "expected ECON-006 call-site-present finding"
    );
}
