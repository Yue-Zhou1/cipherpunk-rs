use project_ir::ProjectIrBuilder;

#[tokio::test]
async fn dataflow_edges_are_redacted_by_default() {
    let fixture = std::path::PathBuf::from("crates/engine-crypto/tests/fixtures/rust-crypto");
    let ir = ProjectIrBuilder::for_path(&fixture)
        .build()
        .await
        .expect("build project ir");
    assert!(
        ir.dataflow_graph
            .edges
            .iter()
            .all(|edge| edge.value_preview.is_none())
    );
}
