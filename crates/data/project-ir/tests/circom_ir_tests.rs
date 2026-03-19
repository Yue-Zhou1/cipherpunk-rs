use std::fs;
use std::path::Path;

use project_ir::ProjectIrBuilder;

fn write_file(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent directory");
    }
    fs::write(path, content).expect("write file");
}

#[tokio::test]
async fn circom_ir_indexes_templates_and_signals_above_file_level() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_file(
        &dir.path().join("circuits/main.circom"),
        r#"
template PoseidonHash() {
    signal input left;
    signal input right;
    signal output out;
    signal scratch;
    out <== left + right + scratch;
}
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
            .any(|node| node.kind == "circom_template" && node.name == "PoseidonHash"),
        "template declarations should be represented as symbol nodes"
    );
    assert!(
        ir.symbol_graph.nodes.iter().any(|node| {
            node.kind == "circom_signal_input"
                && node.name == "PoseidonHash::left"
        }),
        "signal input declarations should be represented as symbol nodes"
    );
    assert!(
        ir.symbol_graph.nodes.iter().any(|node| {
            node.kind == "circom_signal_output"
                && node.name == "PoseidonHash::out"
        }),
        "signal output declarations should be represented as symbol nodes"
    );
    assert!(
        ir.symbol_graph
            .edges
            .iter()
            .any(|edge| edge.relation == "declares_signal"),
        "template-to-signal relationships should be represented in symbol graph edges"
    );
}
