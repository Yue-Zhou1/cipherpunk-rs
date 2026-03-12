use project_ir::{GraphLensKind, ProjectIrBuilder};

#[tokio::test]
async fn rust_fixture_builds_file_and_symbol_graphs() {
    let _lens = GraphLensKind::Symbol;
    let fixture = std::path::PathBuf::from("crates/engine-crypto/tests/fixtures/rust-crypto");
    let ir = ProjectIrBuilder::for_path(&fixture)
        .build()
        .await
        .expect("build project ir");
    assert!(!ir.file_graph.nodes.is_empty());
    assert!(!ir.symbol_graph.nodes.is_empty());
}
