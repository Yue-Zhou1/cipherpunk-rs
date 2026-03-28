use std::path::{Path, PathBuf};

use project_ir::{
    BasicEdge, DataflowEdge, DataflowNode, FileNode, FunctionSignature, Graph, ParameterInfo,
    ProjectIr, SymbolNode,
};

use super::{ExplorerBuildError, ExplorerDepth, ExplorerGraphBuilder, hash_id};

#[test]
fn hash_id_is_deterministic() {
    let id1 = hash_id("sym", "engine-crypto/src/verify.rs::verify_signature");
    let id2 = hash_id("sym", "engine-crypto/src/verify.rs::verify_signature");

    assert_eq!(id1, id2);
    assert!(id1.starts_with("sym_"));
    assert_eq!(id1.len(), 16);
}

#[test]
fn hash_id_different_inputs_differ() {
    let id1 = hash_id("sym", "a.rs::foo");
    let id2 = hash_id("sym", "a.rs::bar");

    assert_ne!(id1, id2);
}

#[test]
fn hash_id_different_prefixes_differ() {
    let id1 = hash_id("crt", "engine-crypto");
    let id2 = hash_id("mod", "engine-crypto");

    assert_ne!(id1, id2);
}

#[test]
fn overview_returns_hierarchy_without_symbols() {
    let root = PathBuf::from("/project");
    let ir = make_test_ir(&root);
    let builder = ExplorerGraphBuilder::new(&ir, &root);

    let response = builder
        .build("sess-1", ExplorerDepth::Overview, None)
        .expect("overview graph should build");

    assert_eq!(response.nodes.len(), 4);
    assert!(response.nodes.iter().all(|node| node.kind != "function"));

    let crate_node = response
        .nodes
        .iter()
        .find(|node| node.kind == "crate")
        .expect("crate node should exist");
    assert!(crate_node.child_count.is_some());
    assert!(crate_node.id.starts_with("crt_"));
}

#[test]
fn full_depth_includes_symbols_with_signatures() {
    let root = PathBuf::from("/project");
    let ir = make_test_ir(&root);
    let builder = ExplorerGraphBuilder::new(&ir, &root);

    let response = builder
        .build("sess-1", ExplorerDepth::Full, None)
        .expect("full graph should build");

    assert_eq!(response.nodes.len(), 5);

    let symbol = response
        .nodes
        .iter()
        .find(|node| node.kind == "function")
        .expect("function symbol should exist");
    assert_eq!(symbol.label, "do_thing");
    assert!(symbol.id.starts_with("sym_"));

    let signature = symbol
        .signature
        .as_ref()
        .expect("function signature should exist");
    assert_eq!(signature.parameters.len(), 1);
    assert_eq!(signature.parameters[0].name, "input");
    assert_eq!(signature.return_type.as_deref(), Some("bool"));
}

#[test]
fn cluster_filter_returns_only_children() {
    let root = PathBuf::from("/project");
    let ir = make_test_ir(&root);
    let builder = ExplorerGraphBuilder::new(&ir, &root);

    let full = builder
        .build("sess-1", ExplorerDepth::Full, None)
        .expect("full graph should build");
    let crate_id = full
        .nodes
        .iter()
        .find(|node| node.kind == "crate")
        .expect("crate node should exist")
        .id
        .clone();

    let filtered = builder
        .build("sess-1", ExplorerDepth::Full, Some(&crate_id))
        .expect("cluster graph should build");

    assert!(filtered.nodes.iter().all(|node| node.id != crate_id));
    assert!(!filtered.nodes.is_empty());
}

#[test]
fn unknown_cluster_returns_error() {
    let root = PathBuf::from("/project");
    let ir = make_test_ir(&root);
    let builder = ExplorerGraphBuilder::new(&ir, &root);

    let result = builder.build("sess-1", ExplorerDepth::Full, Some("missing_cluster"));

    assert!(matches!(
        result,
        Err(ExplorerBuildError::UnknownCluster(value)) if value == "missing_cluster"
    ));
}

fn make_test_ir(root: &Path) -> ProjectIr {
    let file_nodes = vec![
        FileNode {
            id: "file:mycrate/src/lib.rs".to_string(),
            path: root.join("mycrate/src/lib.rs"),
            language: "rust".to_string(),
        },
        FileNode {
            id: "file:mycrate/src/utils.rs".to_string(),
            path: root.join("mycrate/src/utils.rs"),
            language: "rust".to_string(),
        },
    ];

    let symbol_nodes = vec![SymbolNode {
        id: "sym:mycrate/src/lib.rs::do_thing".to_string(),
        name: "do_thing".to_string(),
        qualified_name: Some("mycrate::do_thing".to_string()),
        file: root.join("mycrate/src/lib.rs"),
        kind: "function".to_string(),
        line: 10,
        signature: Some(FunctionSignature {
            parameters: vec![ParameterInfo {
                name: "input".to_string(),
                type_annotation: Some("&[u8]".to_string()),
                position: 0,
            }],
            return_type: Some("bool".to_string()),
        }),
    }];

    ProjectIr {
        file_graph: Graph {
            nodes: file_nodes,
            edges: vec![BasicEdge {
                from: "file:mycrate/src/lib.rs".to_string(),
                to: "file:mycrate/src/utils.rs".to_string(),
                relation: "contains".to_string(),
            }],
        },
        symbol_graph: Graph {
            nodes: symbol_nodes,
            edges: vec![],
        },
        feature_graph: Graph::default(),
        dataflow_graph: Graph {
            nodes: vec![DataflowNode {
                id: "sym:mycrate/src/lib.rs::do_thing".to_string(),
                label: "do_thing".to_string(),
                file: Some(root.join("mycrate/src/lib.rs")),
            }],
            edges: vec![DataflowEdge {
                from: "sym:mycrate/src/lib.rs::do_thing".to_string(),
                to: "sym:mycrate/src/lib.rs::do_thing".to_string(),
                relation: "return_flow".to_string(),
                value_preview: Some("true".to_string()),
            }],
        },
        framework_views: vec![],
    }
}
