use project_ir::{GraphLensKind, ProjectIrBuilder};

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
