use project_ir::{GraphLensKind, ProjectIrBuilder};
use std::fs;
use std::path::Path;

fn write_file(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent directory");
    }
    fs::write(path, content).expect("write file");
}

#[tokio::test]
async fn rust_fixture_builds_file_and_symbol_graphs() {
    let _lens = GraphLensKind::Symbol;
    let fixture = std::path::PathBuf::from("crates/engines/crypto/tests/fixtures/rust-crypto");
    let ir = ProjectIrBuilder::for_path(&fixture)
        .build()
        .await
        .expect("build project ir");
    assert!(!ir.file_graph.nodes.is_empty());
    assert!(!ir.symbol_graph.nodes.is_empty());
}

#[tokio::test]
async fn call_edges_are_scoped_to_the_caller_function() {
    let fixture = std::path::PathBuf::from("crates/engines/crypto/tests/fixtures/rust-crypto");
    let ir = ProjectIrBuilder::for_path(&fixture)
        .build()
        .await
        .expect("build project ir");

    let helper_self_edge = ir.symbol_graph.edges.iter().any(|edge| {
        edge.relation == "calls"
            && edge.from.contains("crypto001_nonce_reuse.rs::aead_encrypt")
            && edge.to.contains("crypto001_nonce_reuse.rs::aead_encrypt")
    });
    assert!(
        !helper_self_edge,
        "helper function should not inherit calls from other functions in the file"
    );
}

#[tokio::test]
async fn rust_ir_surfaces_macro_sites_cfg_divergence_and_trait_impl_methods() {
    let dir = tempfile::tempdir().expect("tempdir");
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
asm = []
"#,
    );
    write_file(
        &dir.path().join("zk/src/lib.rs"),
        r#"
macro_rules! halo2_gate {
    () => { helper() };
}

pub trait Chip {
    fn configure(&self);
}

pub struct RangeCheckChip;

impl Chip for RangeCheckChip {
    #[cfg(feature = "asm")]
    fn configure(&self) {
        halo2_gate!();
    }
}

fn helper() {}
"#,
    );

    let ir = ProjectIrBuilder::for_path(dir.path())
        .build()
        .await
        .expect("build project ir");

    assert!(
        ir.symbol_graph
            .nodes
            .iter()
            .any(|node| node.kind == "macro_call" && node.name == "halo2_gate!"),
        "macro invocation sites should be represented as symbol nodes"
    );
    assert!(
        ir.symbol_graph.nodes.iter().any(|node| {
            node.kind == "trait_impl_method" && node.name == "Chip::configure@RangeCheckChip"
        }),
        "trait impl methods should be represented for downstream halo2 analysis"
    );
    assert!(
        ir.feature_graph.nodes.iter().any(|node| node.name == "asm"),
        "cfg divergence markers should be represented in feature graph"
    );
}
