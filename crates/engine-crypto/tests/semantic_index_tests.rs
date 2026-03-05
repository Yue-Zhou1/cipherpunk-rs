use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::{LazyLock, Mutex};

use audit_agent_core::audit_config::BudgetConfig;
use engine_crypto::semantic::ra_client::{
    SemanticBackend, SemanticIndex, set_semantic_build_delay_for_tests,
};
use intake::workspace::WorkspaceAnalyzer;
use tempfile::tempdir;
use tree_sitter::Parser;

static SEMANTIC_TEST_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

fn semantic_test_lock() -> std::sync::MutexGuard<'static, ()> {
    SEMANTIC_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn write_file(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent directory");
    }
    fs::write(path, content).expect("write file");
}

fn budget_with_timeout(timeout_secs: u64) -> BudgetConfig {
    BudgetConfig {
        kani_timeout_secs: 300,
        z3_timeout_secs: 600,
        fuzz_duration_secs: 3600,
        madsim_ticks: 100_000,
        max_llm_retries: 3,
        semantic_index_timeout_secs: timeout_secs,
    }
}

fn tree_sitter_call_map(source: &str) -> HashMap<String, Vec<String>> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_rust::language())
        .expect("set rust language");
    let tree = parser.parse(source, None).expect("parse source");
    let mut calls = HashMap::<String, Vec<String>>::new();

    fn collect_calls_in_body(node: tree_sitter::Node, source: &str, out: &mut Vec<String>) {
        if node.kind() == "call_expression"
            && let Some(callee_node) = node.child(0)
        {
            out.push(source[callee_node.start_byte()..callee_node.end_byte()].to_string());
        }

        let mut cursor = node.walk();
        if cursor.goto_first_child() {
            loop {
                collect_calls_in_body(cursor.node(), source, out);
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
    }

    fn walk(node: tree_sitter::Node, source: &str, calls: &mut HashMap<String, Vec<String>>) {
        if node.kind() == "function_item"
            && let Some(name_node) = node.child_by_field_name("name")
            && let Some(body_node) = node.child_by_field_name("body")
        {
            let caller = source[name_node.start_byte()..name_node.end_byte()].to_string();
            let mut callee_calls = Vec::<String>::new();
            collect_calls_in_body(body_node, source, &mut callee_calls);
            calls.insert(caller, callee_calls);
        }

        let mut cursor = node.walk();
        if cursor.goto_first_child() {
            loop {
                walk(cursor.node(), source, calls);
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
    }

    walk(tree.root_node(), source, &mut calls);
    calls
}

#[tokio::test]
async fn semantic_index_finds_chip_configure_impl_across_workspace_members() {
    let dir = tempdir().expect("tempdir");

    write_file(
        &dir.path().join("Cargo.toml"),
        r#"[workspace]
members = ["halo2-api", "halo2-chip"]
resolver = "2"
"#,
    );

    write_file(
        &dir.path().join("halo2-api/Cargo.toml"),
        r#"[package]
name = "halo2-api"
version = "0.1.0"
edition = "2024"
"#,
    );
    write_file(
        &dir.path().join("halo2-api/src/lib.rs"),
        r#"pub trait Chip {
    fn configure();
}
"#,
    );

    write_file(
        &dir.path().join("halo2-chip/Cargo.toml"),
        r#"[package]
name = "halo2-chip"
version = "0.1.0"
edition = "2024"

[dependencies]
halo2-api = { path = "../halo2-api" }
"#,
    );
    write_file(
        &dir.path().join("halo2-chip/src/lib.rs"),
        r#"use halo2_api::Chip;

pub struct RangeCheckChip;

impl Chip for RangeCheckChip {
    fn configure() {}
}
"#,
    );

    let workspace = WorkspaceAnalyzer::analyze(dir.path()).expect("analyze workspace");
    let index = SemanticIndex::build(&workspace, &budget_with_timeout(5))
        .await
        .expect("build semantic index");

    let impls = index.find_trait_impls("Chip", "configure");
    assert!(
        impls.iter().any(|f| f.impl_type == "RangeCheckChip"),
        "expected Chip::configure impl for RangeCheckChip"
    );
    assert!(matches!(
        index.backend,
        SemanticBackend::RustAnalyzer { .. } | SemanticBackend::LspSubprocess { .. }
    ));
}

#[tokio::test]
async fn semantic_index_detects_cfg_feature_divergence_points() {
    let dir = tempdir().expect("tempdir");
    write_file(
        &dir.path().join("Cargo.toml"),
        r#"[workspace]
members = ["zk"]
resolver = "2"
"#,
    );
    write_file(
        &dir.path().join("zk/Cargo.toml"),
        r#"[package]
name = "zk"
version = "0.1.0"
edition = "2024"

[features]
default = []
asm = []
"#,
    );
    write_file(
        &dir.path().join("zk/src/lib.rs"),
        r#"#[cfg(feature = "asm")]
pub fn field_mul_asm() -> u64 { 1 }

pub fn field_mul_generic() -> u64 { 2 }
"#,
    );

    let workspace = WorkspaceAnalyzer::analyze(dir.path()).expect("analyze workspace");
    let index = SemanticIndex::build(&workspace, &budget_with_timeout(5))
        .await
        .expect("build semantic index");

    let divergences = index.cfg_divergence_points();
    assert!(
        divergences
            .iter()
            .any(|point| point.feature == "asm" && point.crate_name == "zk"),
        "expected asm cfg divergence point"
    );
}

#[tokio::test]
async fn semantic_index_degrades_to_tree_sitter_fallback_on_timeout() {
    let _guard = semantic_test_lock();
    let dir = tempdir().expect("tempdir");
    write_file(
        &dir.path().join("Cargo.toml"),
        r#"[workspace]
members = ["zk"]
resolver = "2"
"#,
    );
    write_file(
        &dir.path().join("zk/Cargo.toml"),
        r#"[package]
name = "zk"
version = "0.1.0"
edition = "2024"
"#,
    );
    write_file(&dir.path().join("zk/src/lib.rs"), "pub fn x() {}\n");

    let workspace = WorkspaceAnalyzer::analyze(dir.path()).expect("analyze workspace");
    set_semantic_build_delay_for_tests(1500);
    let index = SemanticIndex::build(&workspace, &budget_with_timeout(1))
        .await
        .expect("build semantic index");
    set_semantic_build_delay_for_tests(0);

    match &index.backend {
        SemanticBackend::TreeSitterFallback { reason } => {
            assert!(reason.contains("timed out"));
        }
        backend => panic!("expected fallback backend, got {backend:?}"),
    }
}

#[tokio::test(flavor = "current_thread")]
async fn semantic_index_build_timeout_does_not_block_current_thread_runtime() {
    let _guard = semantic_test_lock();
    let dir = tempdir().expect("tempdir");
    write_file(
        &dir.path().join("Cargo.toml"),
        r#"[workspace]
members = ["zk"]
resolver = "2"
"#,
    );
    write_file(
        &dir.path().join("zk/Cargo.toml"),
        r#"[package]
name = "zk"
version = "0.1.0"
edition = "2024"
"#,
    );
    write_file(&dir.path().join("zk/src/lib.rs"), "pub fn x() {}\n");

    let workspace = WorkspaceAnalyzer::analyze(dir.path()).expect("analyze workspace");
    let (tick_tx, tick_rx) = std::sync::mpsc::channel::<()>();
    tokio::spawn(async move {
        let _ = tick_tx.send(());
    });

    set_semantic_build_delay_for_tests(1500);
    let index = SemanticIndex::build(&workspace, &budget_with_timeout(1))
        .await
        .expect("build semantic index");
    set_semantic_build_delay_for_tests(0);

    assert!(
        tick_rx.try_recv().is_ok(),
        "runtime task should run while semantic index waits for timeout"
    );
    assert!(matches!(
        index.backend,
        SemanticBackend::TreeSitterFallback { .. }
    ));
}

#[tokio::test]
async fn semantic_index_falls_back_to_lsp_subprocess_when_ra_is_forced_to_fail() {
    let _guard = semantic_test_lock();
    let dir = tempdir().expect("tempdir");
    write_file(
        &dir.path().join("Cargo.toml"),
        r#"[workspace]
members = ["zk"]
resolver = "2"
"#,
    );
    write_file(
        &dir.path().join("zk/Cargo.toml"),
        r#"[package]
name = "zk"
version = "0.1.0"
edition = "2024"
"#,
    );
    write_file(&dir.path().join("zk/src/lib.rs"), "pub fn x() {}\n");

    let workspace = WorkspaceAnalyzer::analyze(dir.path()).expect("analyze workspace");
    // SAFETY: guarded by a process-wide mutex so no concurrent env mutation happens in tests.
    unsafe { std::env::set_var("AUDIT_AGENT_SEMANTIC_FORCE_RA_FAIL", "1") };
    let index = SemanticIndex::build(&workspace, &budget_with_timeout(5))
        .await
        .expect("build semantic index");
    // SAFETY: guarded by a process-wide mutex so no concurrent env mutation happens in tests.
    unsafe { std::env::remove_var("AUDIT_AGENT_SEMANTIC_FORCE_RA_FAIL") };

    assert!(matches!(
        index.backend,
        SemanticBackend::LspSubprocess { .. }
    ));
}

#[test]
fn semantic_index_records_backend_variant_in_manifest_tool_versions() {
    let mut versions = HashMap::new();
    SemanticBackend::TreeSitterFallback {
        reason: "semantic index timed out".to_string(),
    }
    .record_in_tool_versions(&mut versions);

    assert_eq!(
        versions.get("semantic_backend"),
        Some(&"tree-sitter-fallback".to_string())
    );
    assert_eq!(
        versions.get("semantic_backend_reason"),
        Some(&"semantic index timed out".to_string())
    );
}

#[tokio::test]
async fn semantic_call_graph_includes_macro_expansion_edges_missing_in_tree_sitter_graph() {
    let dir = tempdir().expect("tempdir");
    write_file(
        &dir.path().join("Cargo.toml"),
        r#"[workspace]
members = ["zk"]
resolver = "2"
"#,
    );
    write_file(
        &dir.path().join("zk/Cargo.toml"),
        r#"[package]
name = "zk"
version = "0.1.0"
edition = "2024"
"#,
    );
    let source = r#"
macro_rules! halo2_gate {
    () => { 1 + 1 };
}

pub fn configure() {
    let _ = halo2_gate!();
}
"#;
    write_file(&dir.path().join("zk/src/lib.rs"), source);

    let workspace = WorkspaceAnalyzer::analyze(dir.path()).expect("analyze workspace");
    let index = SemanticIndex::build(&workspace, &budget_with_timeout(5))
        .await
        .expect("build semantic index");

    let ts_graph = tree_sitter_call_map(source);
    assert!(
        !ts_graph.contains_key("__macro_root__"),
        "tree-sitter call graph should not expose macro expansion root"
    );
    let semantic_macro_calls = index
        .call_graph
        .get("__macro_root__")
        .cloned()
        .unwrap_or_default();
    assert!(
        semantic_macro_calls.contains("halo2_gate!"),
        "semantic graph should include macro expansion edge"
    );
}

#[tokio::test]
async fn semantic_index_ignores_braces_in_comments_when_tracking_fn_context() {
    let dir = tempdir().expect("tempdir");
    write_file(
        &dir.path().join("Cargo.toml"),
        r#"[workspace]
members = ["zk"]
resolver = "2"
"#,
    );
    write_file(
        &dir.path().join("zk/Cargo.toml"),
        r#"[package]
name = "zk"
version = "0.1.0"
edition = "2024"
"#,
    );
    write_file(
        &dir.path().join("zk/src/lib.rs"),
        r#"
pub trait Chip {
    fn configure();
}

pub struct CommentChip;

impl Chip for CommentChip {
    fn configure() {
        // } this comment must not close fn context
        helper();
    }
}

fn helper() {}
"#,
    );

    let workspace = WorkspaceAnalyzer::analyze(dir.path()).expect("analyze workspace");
    let index = SemanticIndex::build(&workspace, &budget_with_timeout(5))
        .await
        .expect("build semantic index");

    let configure_calls = index
        .call_graph
        .get("configure")
        .cloned()
        .unwrap_or_default();
    assert!(
        configure_calls.contains("helper"),
        "helper call should still be attributed to configure"
    );
}

#[tokio::test]
async fn semantic_index_ignores_braces_in_strings_when_tracking_fn_context() {
    let dir = tempdir().expect("tempdir");
    write_file(
        &dir.path().join("Cargo.toml"),
        r#"[workspace]
members = ["zk"]
resolver = "2"
"#,
    );
    write_file(
        &dir.path().join("zk/Cargo.toml"),
        r#"[package]
name = "zk"
version = "0.1.0"
edition = "2024"
"#,
    );
    write_file(
        &dir.path().join("zk/src/lib.rs"),
        r#"
fn configure() {
    let _note = "hello {world";
}

fn downstream() {
    sink();
}

fn sink() {}
"#,
    );

    let workspace = WorkspaceAnalyzer::analyze(dir.path()).expect("analyze workspace");
    let index = SemanticIndex::build(&workspace, &budget_with_timeout(5))
        .await
        .expect("build semantic index");

    let configure_calls = index
        .call_graph
        .get("configure")
        .cloned()
        .unwrap_or_default();
    let downstream_calls = index
        .call_graph
        .get("downstream")
        .cloned()
        .unwrap_or_default();
    assert!(
        !configure_calls.contains("sink"),
        "sink call should not leak into configure context"
    );
    assert!(
        downstream_calls.contains("sink"),
        "sink call should remain attributed to downstream"
    );
}
