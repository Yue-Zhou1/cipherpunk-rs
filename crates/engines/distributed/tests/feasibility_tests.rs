use std::fs;
use std::path::Path;

use audit_agent_core::audit_config::BudgetConfig;
use engine_crypto::semantic::ra_client::SemanticIndex;
use engine_distributed::feasibility::{BridgeLevel, MadSimFeasibilityAssessor};
use intake::workspace::WorkspaceAnalyzer;
use tempfile::tempdir;

fn write_file(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent directory");
    }
    fs::write(path, content).expect("write file");
}

fn budget() -> BudgetConfig {
    BudgetConfig {
        kani_timeout_secs: 300,
        z3_timeout_secs: 600,
        fuzz_duration_secs: 3600,
        madsim_ticks: 100_000,
        max_llm_retries: 3,
        semantic_index_timeout_secs: 5,
    }
}

#[tokio::test]
async fn feasibility_classifies_simple_project_as_level_a() {
    let dir = tempdir().expect("tempdir");
    write_file(
        &dir.path().join("Cargo.toml"),
        r#"[workspace]
members = ["echo"]
resolver = "2"
"#,
    );
    write_file(
        &dir.path().join("echo/Cargo.toml"),
        r#"[package]
name = "echo"
version = "0.1.0"
edition = "2024"
"#,
    );
    write_file(
        &dir.path().join("echo/src/lib.rs"),
        r#"pub async fn start_node() {}
"#,
    );

    let workspace = WorkspaceAnalyzer::analyze(dir.path()).expect("analyze workspace");
    let index = SemanticIndex::build(&workspace, &budget())
        .await
        .expect("build semantic index");

    let level = MadSimFeasibilityAssessor::assess(&workspace, &index);
    assert_eq!(level, BridgeLevel::LevelA);
}

#[tokio::test]
async fn feasibility_classifies_libp2p_project_as_level_b_with_adapter_points() {
    let dir = tempdir().expect("tempdir");
    write_file(
        &dir.path().join("Cargo.toml"),
        r#"[workspace]
members = ["p2p-node"]
resolver = "2"
"#,
    );
    write_file(
        &dir.path().join("p2p-node/Cargo.toml"),
        r#"[package]
name = "p2p-node"
version = "0.1.0"
edition = "2024"

[dependencies]
libp2p = "0.53"
"#,
    );
    write_file(
        &dir.path().join("p2p-node/src/lib.rs"),
        r#"use libp2p::Swarm;

pub fn bootstrap() {
    let _ = Swarm::<()>::new;
}
"#,
    );

    let workspace = WorkspaceAnalyzer::analyze(dir.path()).expect("analyze workspace");
    let index = SemanticIndex::build(&workspace, &budget())
        .await
        .expect("build semantic index");

    let level = MadSimFeasibilityAssessor::assess(&workspace, &index);

    match level {
        BridgeLevel::LevelB { adapter_points } => {
            assert!(!adapter_points.is_empty(), "expected adapter points");
            assert!(
                adapter_points
                    .iter()
                    .any(|point| point.reason.contains("libp2p")),
                "expected at least one libp2p adapter point"
            );
        }
        other => panic!("expected LevelB, got {other:?}"),
    }
}

#[tokio::test]
async fn feasibility_classifies_scattered_tokio_runtime_creation_as_level_c() {
    let dir = tempdir().expect("tempdir");
    write_file(
        &dir.path().join("Cargo.toml"),
        r#"[workspace]
members = ["runtime-fragmented"]
resolver = "2"
"#,
    );
    write_file(
        &dir.path().join("runtime-fragmented/Cargo.toml"),
        r#"[package]
name = "runtime-fragmented"
version = "0.1.0"
edition = "2024"
"#,
    );
    write_file(
        &dir.path().join("runtime-fragmented/src/lib.rs"),
        r#"pub fn boot() {
    let _ = tokio::runtime::Runtime::new();
}
"#,
    );
    write_file(
        &dir.path().join("runtime-fragmented/src/worker.rs"),
        r#"pub fn run_worker() {
    let _ = tokio::runtime::Runtime::new();
}
"#,
    );

    let workspace = WorkspaceAnalyzer::analyze(dir.path()).expect("analyze workspace");
    let index = SemanticIndex::build(&workspace, &budget())
        .await
        .expect("build semantic index");

    let level = MadSimFeasibilityAssessor::assess(&workspace, &index);

    match level {
        BridgeLevel::LevelC { reason } => {
            assert!(
                reason.contains("tokio::Runtime::new"),
                "reason should mention fragmented runtime ownership"
            );
        }
        other => panic!("expected LevelC, got {other:?}"),
    }
}
