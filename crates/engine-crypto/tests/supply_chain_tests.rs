use std::fs;
use std::path::Path;

use audit_agent_core::finding::Severity;
use engine_crypto::supply_chain::{CargoAuditAdvisory, DependencyKind, SupplyChainAnalyzer};
use intake::workspace::WorkspaceAnalyzer;
use tempfile::tempdir;

fn write_file(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent dir");
    }
    fs::write(path, content).expect("write file");
}

#[tokio::test]
async fn escalates_reachable_curve25519_advisory_and_includes_call_chain() {
    let dir = tempdir().expect("tempdir");
    write_file(
        &dir.path().join("Cargo.toml"),
        r#"
[workspace]
members = ["crypto-app"]
"#,
    );
    write_file(
        &dir.path().join("crypto-app/Cargo.toml"),
        r#"
[package]
name = "crypto-app"
version = "0.1.0"
edition = "2024"
"#,
    );
    write_file(
        &dir.path().join("crypto-app/src/lib.rs"),
        r#"
pub fn sign_data() {
    signing_core();
}

fn signing_core() {
    curve25519_vulnerable();
}

fn curve25519_vulnerable() {}

pub fn helper_not_crypto() {
    harmless_fn();
}

fn harmless_fn() {}
"#,
    );

    let workspace = WorkspaceAnalyzer::analyze(dir.path()).expect("analyze workspace");
    let advisories = vec![
        CargoAuditAdvisory {
            cve_id: "CVE-2026-0001".to_string(),
            crate_name: "curve25519-dalek".to_string(),
            affected_fn: "curve25519_vulnerable".to_string(),
            severity: Severity::Medium,
            dependency_kind: DependencyKind::Normal,
        },
        CargoAuditAdvisory {
            cve_id: "CVE-2026-0002".to_string(),
            crate_name: "dev-only-crate".to_string(),
            affected_fn: "dev_only_fn".to_string(),
            severity: Severity::High,
            dependency_kind: DependencyKind::Dev,
        },
    ];

    let analyzer = SupplyChainAnalyzer::tree_sitter(advisories);
    let results = analyzer.analyze(&workspace).await.expect("analyze");

    let reachable = results
        .iter()
        .find(|r| r.cve_id == "CVE-2026-0001")
        .expect("reachable result");
    assert!(reachable.reachable_from_crypto_path);
    assert!(
        matches!(reachable.adjusted_severity, Severity::Critical | Severity::High),
        "reachable advisory should escalate to High or Critical"
    );
    assert!(
        !reachable.call_chain.is_empty(),
        "call chain should be recorded"
    );
    assert!(
        matches!(
            reachable.call_chain.first().map(|s| s.as_str()),
            Some("sign_data") | Some("signing_core")
        ),
        "call chain should begin from a signing path"
    );
    assert_eq!(
        reachable.call_chain.last().map(|s| s.as_str()),
        Some("curve25519_vulnerable")
    );

    let dev = results
        .iter()
        .find(|r| r.cve_id == "CVE-2026-0002")
        .expect("dev result");
    assert_eq!(dev.adjusted_severity, Severity::Low);
}

#[tokio::test]
async fn records_tree_sitter_backend_on_every_supply_chain_result() {
    let dir = tempdir().expect("tempdir");
    write_file(
        &dir.path().join("Cargo.toml"),
        r#"
[workspace]
members = ["app"]
"#,
    );
    write_file(
        &dir.path().join("app/Cargo.toml"),
        r#"
[package]
name = "app"
version = "0.1.0"
edition = "2024"
"#,
    );
    write_file(
        &dir.path().join("app/src/lib.rs"),
        r#"
pub fn verify_data() {
    target_fn();
}

fn target_fn() {}
"#,
    );
    let workspace = WorkspaceAnalyzer::analyze(dir.path()).expect("analyze workspace");

    let analyzer = SupplyChainAnalyzer::tree_sitter(vec![CargoAuditAdvisory {
        cve_id: "CVE-2026-0100".to_string(),
        crate_name: "dep".to_string(),
        affected_fn: "target_fn".to_string(),
        severity: Severity::Medium,
        dependency_kind: DependencyKind::Normal,
    }]);
    let results = analyzer.analyze(&workspace).await.expect("analyze");
    assert_eq!(results.len(), 1);
    assert!(results.iter().all(|r| r.graph_backend == "tree-sitter"));
}
