use std::fs;
use std::path::Path;

use project_ir::ProjectIrBuilder;
use tempfile::TempDir;

fn write_file(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent directory");
    }
    fs::write(path, content).expect("write file");
}

fn build_two_member_workspace() -> TempDir {
    let root = tempfile::tempdir().expect("create workspace tempdir");
    write_file(
        &root.path().join("Cargo.toml"),
        r#"
[workspace]
members = ["alpha", "beta"]
resolver = "2"
"#,
    );

    write_file(
        &root.path().join("alpha/Cargo.toml"),
        r#"
[package]
name = "alpha"
version = "0.1.0"
edition = "2021"

[features]
default = ["alpha-fast-path"]
alpha-fast-path = []
"#,
    );
    write_file(
        &root.path().join("alpha/src/lib.rs"),
        "pub fn alpha_encrypt() -> bool { true }\n",
    );

    write_file(
        &root.path().join("beta/Cargo.toml"),
        r#"
[package]
name = "beta"
version = "0.1.0"
edition = "2021"

[features]
beta-hardened = []
"#,
    );
    write_file(
        &root.path().join("beta/src/lib.rs"),
        "pub fn beta_verify() -> bool { true }\n",
    );

    root
}

#[tokio::test]
async fn cargo_workspace_members_and_manifest_features_flow_into_project_ir() {
    let workspace = build_two_member_workspace();
    let root = workspace.path();

    let ir = ProjectIrBuilder::for_path(root)
        .build()
        .await
        .expect("build project ir from workspace");

    let alpha_file = root.join("alpha/src/lib.rs");
    let beta_file = root.join("beta/src/lib.rs");
    assert!(
        ir.file_graph
            .nodes
            .iter()
            .any(|node| node.path == alpha_file),
        "expected alpha member file to be indexed"
    );
    assert!(
        ir.file_graph
            .nodes
            .iter()
            .any(|node| node.path == beta_file),
        "expected beta member file to be indexed"
    );

    assert!(
        ir.feature_graph
            .nodes
            .iter()
            .any(|node| node.name == "alpha-fast-path"),
        "expected member manifest feature to appear in feature graph"
    );
    assert!(
        ir.feature_graph
            .nodes
            .iter()
            .any(|node| node.name == "beta-hardened"),
        "expected second member manifest feature to appear in feature graph"
    );
}

#[tokio::test]
async fn plain_directory_without_cargo_manifest_still_builds_ir() {
    let root = tempfile::tempdir().expect("create plain directory");
    write_file(
        &root.path().join("src/lib.rs"),
        "pub fn standalone() -> usize { 42 }\n",
    );

    let ir = ProjectIrBuilder::for_path(root.path())
        .build()
        .await
        .expect("build project ir for plain directory");

    assert!(
        !ir.file_graph.nodes.is_empty(),
        "plain directory fallback should produce file graph nodes"
    );
    assert!(
        ir.file_graph
            .nodes
            .iter()
            .any(|node| node.path == root.path().join("src/lib.rs")),
        "fallback graph should include the standalone rust file"
    );
}

#[tokio::test]
async fn manifest_and_cfg_feature_nodes_are_deduplicated() {
    let root = tempfile::tempdir().expect("create workspace tempdir");
    write_file(
        &root.path().join("Cargo.toml"),
        r#"
[workspace]
members = ["alpha"]
resolver = "2"
"#,
    );
    write_file(
        &root.path().join("alpha/Cargo.toml"),
        r#"
[package]
name = "alpha"
version = "0.1.0"
edition = "2021"

[features]
fast-path = []
"#,
    );
    write_file(
        &root.path().join("alpha/src/lib.rs"),
        r#"
#[cfg(feature = "fast-path")]
pub fn run() {}
"#,
    );

    let ir = ProjectIrBuilder::for_path(root.path())
        .build()
        .await
        .expect("build project ir from workspace");

    let feature_count = ir
        .feature_graph
        .nodes
        .iter()
        .filter(|node| node.name == "fast-path")
        .count();
    assert_eq!(
        feature_count, 1,
        "manifest and source cfg should map to one feature node"
    );
}

#[tokio::test]
async fn member_subdirectory_input_is_treated_as_single_member_workspace() {
    let workspace = build_two_member_workspace();
    let alpha_root = workspace.path().join("alpha");

    let ir = ProjectIrBuilder::for_path(&alpha_root)
        .build()
        .await
        .expect("build project ir from member subdirectory");

    assert!(
        ir.file_graph
            .nodes
            .iter()
            .any(|node| node.path.ends_with("alpha/src/lib.rs")),
        "member subdirectory should still index member source files"
    );
    assert!(
        !ir.file_graph
            .nodes
            .iter()
            .any(|node| node.path.ends_with("beta/src/lib.rs")),
        "member subdirectory input should not implicitly widen to sibling members"
    );
}
