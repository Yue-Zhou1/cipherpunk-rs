use std::fs;
use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use audit_agent_core::workspace::CargoWorkspace;
use engine_distributed::feasibility::AdapterPoint;
use engine_distributed::harness::builder::{
    AdapterScaffold, DistributedAuditConfig, HarnessBuilder, NetworkTopology,
};
use intake::detection::{DetectedEntryPoint, EntryPointKind};
use intake::workspace::WorkspaceAnalyzer;
use llm::provider::{CompletionOpts, LlmProvider};
use tempfile::tempdir;

fn write_file(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent directory");
    }
    fs::write(path, content).expect("write file");
}

fn fixture_workspace() -> (tempfile::TempDir, CargoWorkspace) {
    let dir = tempdir().expect("tempdir");
    write_file(
        &dir.path().join("Cargo.toml"),
        r#"[workspace]
members = ["consensus-node"]
resolver = "2"
"#,
    );
    write_file(
        &dir.path().join("consensus-node/Cargo.toml"),
        r#"[package]
name = "consensus-node"
version = "0.1.0"
edition = "2024"
"#,
    );
    write_file(
        &dir.path().join("consensus-node/src/lib.rs"),
        r#"#[derive(Clone, Default)]
pub struct NodeConfig {
    pub id: usize,
}

pub async fn start_node(_cfg: NodeConfig) {}
"#,
    );

    let workspace = WorkspaceAnalyzer::analyze(dir.path()).expect("analyze workspace");
    (dir, workspace)
}

fn entry_points(root: &Path) -> Vec<DetectedEntryPoint> {
    vec![DetectedEntryPoint {
        function: "start_node".to_string(),
        crate_name: "consensus-node".to_string(),
        file: root.join("consensus-node/src/lib.rs"),
        line: 6,
        kind: EntryPointKind::Unknown,
    }]
}

#[tokio::test]
async fn level_a_without_llm_uses_todo_and_smoke_test_runs() {
    let (_dir, workspace) = fixture_workspace();
    let builder = HarnessBuilder::without_llm_for_tests();
    let config = DistributedAuditConfig {
        node_count: 3,
        topology: NetworkTopology::Mesh,
        simulation_duration_secs: 1,
    };

    let harness = builder
        .generate_level_a(&workspace, &entry_points(&workspace.root), &config)
        .await
        .expect("generate level a harness");

    assert!(harness.source().contains("// TODO: fill entry point call"));
    harness
        .run_smoke_test()
        .expect("harness should compile and run");
}

#[tokio::test]
async fn level_b_scaffold_lists_adapter_points_with_file_line_locations() {
    let (_dir, workspace) = fixture_workspace();
    let builder = HarnessBuilder::without_llm_for_tests();
    let points = vec![
        AdapterPoint {
            crate_name: "consensus-node".to_string(),
            file: workspace.root.join("consensus-node/src/network.rs"),
            line: 42,
            reason: "libp2p adapter candidate".to_string(),
        },
        AdapterPoint {
            crate_name: "consensus-node".to_string(),
            file: workspace.root.join("consensus-node/src/gossip.rs"),
            line: 17,
            reason: "gossipsub adapter candidate".to_string(),
        },
    ];

    let scaffold: AdapterScaffold = builder
        .generate_level_b_scaffold(&workspace, &points)
        .await
        .expect("generate level b scaffold");

    assert!(
        scaffold
            .description
            .contains("consensus-node/src/network.rs:42"),
        "expected file:line location in scaffold description"
    );
    assert!(
        scaffold
            .description
            .contains("consensus-node/src/gossip.rs:17"),
        "expected file:line location in scaffold description"
    );
}

struct MockLlmProvider {
    response: String,
}

#[async_trait]
impl LlmProvider for MockLlmProvider {
    async fn complete(&self, _prompt: &str, _opts: &CompletionOpts) -> Result<String> {
        Ok(self.response.clone())
    }

    fn name(&self) -> &str {
        "mock"
    }

    fn is_available(&self) -> bool {
        true
    }
}

#[tokio::test]
async fn level_a_with_llm_filters_entry_call_to_first_spawn_line() {
    let (_dir, workspace) = fixture_workspace();
    let llm = Arc::new(MockLlmProvider {
        response: r#"let cfg = NodeConfig::default();
node_handle.spawn(async move { start_node(cfg.clone()).await; });
node_handle.spawn(async move { panic!("should be dropped"); });"#
            .to_string(),
    });
    let builder = HarnessBuilder::new_with_llm_for_tests(llm);
    let config = DistributedAuditConfig::default();

    let harness = builder
        .generate_level_a(&workspace, &entry_points(&workspace.root), &config)
        .await
        .expect("generate level a harness");

    let source = harness.source();
    assert!(
        source.contains("node_handle.spawn(async move { start_node(cfg.clone()).await; });"),
        "expected first spawn line to be retained"
    );
    assert!(
        !source.contains("should be dropped"),
        "expected non-first spawn lines to be removed"
    );
    assert_eq!(source.matches("spawn(async").count(), 1);
}

#[tokio::test]
async fn generated_harness_block_on_has_timeout_guard() {
    let (_dir, workspace) = fixture_workspace();
    let builder = HarnessBuilder::without_llm_for_tests();
    let harness = builder
        .generate_level_a(
            &workspace,
            &entry_points(&workspace.root),
            &DistributedAuditConfig::default(),
        )
        .await
        .expect("generate level a harness");

    let source = harness.source();
    assert!(
        source.contains("timed out while awaiting harness completion"),
        "generated block_on should include timeout guard to prevent infinite spin"
    );
}
