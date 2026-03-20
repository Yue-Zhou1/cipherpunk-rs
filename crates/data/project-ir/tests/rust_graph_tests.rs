use project_ir::{
    BasicEdge, ContextSnippet, DataflowNode, FileNode, Graph, GraphLensKind, ProjectIr,
    ProjectIrBuilder, SymbolNode,
};
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

#[tokio::test]
async fn rust_ir_surfaces_variable_nodes_and_parameter_return_dataflow_edges() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_file(
        &dir.path().join("Cargo.toml"),
        r#"[workspace]
members = ["demo"]
resolver = "2"
"#,
    );
    write_file(
        &dir.path().join("demo/Cargo.toml"),
        r#"[package]
name = "demo"
version = "0.1.0"
edition = "2024"
"#,
    );
    write_file(
        &dir.path().join("demo/src/lib.rs"),
        r#"
const MAX_RETRIES: usize = 3;

fn callee(input: u32) -> u32 {
    let doubled = input * 2;
    doubled
}

fn caller(seed: u32) -> u32 {
    let local = callee(seed);
    local + 1
}
"#,
    );

    let ir = ProjectIrBuilder::for_path(dir.path())
        .build()
        .await
        .expect("build project ir");

    let caller_symbol = ir
        .symbol_graph
        .nodes
        .iter()
        .find(|node| node.kind == "function" && node.name == "caller")
        .expect("caller symbol");
    let callee_symbol = ir
        .symbol_graph
        .nodes
        .iter()
        .find(|node| node.kind == "function" && node.name == "callee")
        .expect("callee symbol");

    assert_eq!(caller_symbol.line, 9);
    let callee_signature = callee_symbol.signature.as_ref().expect("callee signature");
    assert_eq!(callee_signature.parameters.len(), 1);
    assert_eq!(callee_signature.parameters[0].name, "input");
    assert_eq!(
        callee_signature.return_type.as_deref(),
        Some("u32"),
        "function signature return type should be captured"
    );

    assert!(
        ir.dataflow_graph
            .nodes
            .iter()
            .any(|node| node.label == "MAX_RETRIES"),
        "const declarations should become dataflow nodes"
    );
    assert!(
        ir.dataflow_graph
            .nodes
            .iter()
            .any(|node| node.label == "doubled"),
        "local let declarations should become dataflow nodes"
    );
    assert!(
        ir.dataflow_graph
            .nodes
            .iter()
            .any(|node| node.label == "local"),
        "caller-local let declarations should become dataflow nodes"
    );

    let caller_dataflow_id = format!("dataflow:{}", caller_symbol.id);
    let callee_dataflow_id = format!("dataflow:{}", callee_symbol.id);
    assert!(
        ir.dataflow_graph.edges.iter().any(|edge| {
            edge.from == caller_dataflow_id
                && edge.to == callee_dataflow_id
                && edge.relation == "parameter_flow"
        }),
        "call edges should emit parameter_flow relation"
    );
    assert!(
        ir.dataflow_graph.edges.iter().any(|edge| {
            edge.from == callee_dataflow_id
                && edge.to == caller_dataflow_id
                && edge.relation == "return_flow"
        }),
        "call edges should emit return_flow relation"
    );
    assert!(
        ir.dataflow_graph
            .edges
            .iter()
            .all(|edge| edge.relation != "call-arg-flow"),
        "legacy placeholder relation should be removed"
    );
}

#[test]
fn ir_neighborhood_is_bounded_and_deduplicated_in_deterministic_order() {
    let graph = ProjectIr {
        file_graph: Graph {
            nodes: vec![
                FileNode {
                    id: "file:a".to_string(),
                    path: "a.rs".into(),
                    language: "rust".to_string(),
                },
                FileNode {
                    id: "file:b".to_string(),
                    path: "b.rs".into(),
                    language: "rust".to_string(),
                },
                FileNode {
                    id: "file:c".to_string(),
                    path: "c.rs".into(),
                    language: "rust".to_string(),
                },
                FileNode {
                    id: "file:d".to_string(),
                    path: "d.rs".into(),
                    language: "rust".to_string(),
                },
            ],
            edges: vec![
                BasicEdge {
                    from: "file:a".to_string(),
                    to: "file:b".to_string(),
                    relation: "depends_on".to_string(),
                },
                BasicEdge {
                    from: "file:b".to_string(),
                    to: "file:c".to_string(),
                    relation: "depends_on".to_string(),
                },
                BasicEdge {
                    from: "file:c".to_string(),
                    to: "file:d".to_string(),
                    relation: "depends_on".to_string(),
                },
            ],
        },
        ..ProjectIr::default()
    };

    let seeds = vec!["file:a".to_string(), "file:a".to_string()];
    let neighborhood = graph.ir_neighborhood(&seeds, 3, 8);
    assert_eq!(
        neighborhood,
        vec![
            "file:a".to_string(),
            "file:b".to_string(),
            "file:c".to_string()
        ],
        "bounded neighborhood should keep deterministic BFS order and drop duplicates"
    );
}

#[test]
fn ir_neighborhood_traverses_edges_bidirectionally() {
    let graph = ProjectIr {
        file_graph: Graph {
            nodes: vec![
                FileNode {
                    id: "file:a".to_string(),
                    path: "a.rs".into(),
                    language: "rust".to_string(),
                },
                FileNode {
                    id: "file:b".to_string(),
                    path: "b.rs".into(),
                    language: "rust".to_string(),
                },
            ],
            edges: vec![BasicEdge {
                from: "file:b".to_string(),
                to: "file:a".to_string(),
                relation: "depends_on".to_string(),
            }],
        },
        ..ProjectIr::default()
    };

    let neighborhood = graph.ir_neighborhood(&["file:a".to_string()], 4, 2);
    assert_eq!(
        neighborhood,
        vec!["file:a".to_string(), "file:b".to_string()],
        "neighborhood traversal should treat graph edges as reachable from either endpoint"
    );
}

#[test]
fn context_snippets_respect_budget_and_return_source_backed_entries() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file_a = dir.path().join("a.rs");
    let file_b = dir.path().join("b.rs");
    write_file(
        &file_a,
        "fn alpha() {\n    println!(\"alpha\");\n    println!(\"beta\");\n}\n",
    );
    write_file(
        &file_b,
        "fn bravo() {\n    println!(\"bravo\");\n    println!(\"charlie\");\n}\n",
    );

    let graph = ProjectIr {
        file_graph: Graph {
            nodes: vec![
                FileNode {
                    id: format!("file:{}", file_a.display()),
                    path: file_a.clone(),
                    language: "rust".to_string(),
                },
                FileNode {
                    id: format!("file:{}", file_b.display()),
                    path: file_b.clone(),
                    language: "rust".to_string(),
                },
            ],
            edges: vec![],
        },
        symbol_graph: Graph {
            nodes: vec![
                SymbolNode {
                    id: format!("symbol:{}::alpha", file_a.display()),
                    name: "alpha".to_string(),
                    qualified_name: Some("alpha".to_string()),
                    file: file_a.clone(),
                    kind: "function".to_string(),
                    line: 1,
                    signature: None,
                },
                SymbolNode {
                    id: format!("symbol:{}::bravo", file_b.display()),
                    name: "bravo".to_string(),
                    qualified_name: Some("bravo".to_string()),
                    file: file_b.clone(),
                    kind: "function".to_string(),
                    line: 1,
                    signature: None,
                },
            ],
            edges: vec![],
        },
        dataflow_graph: Graph {
            nodes: vec![DataflowNode {
                id: "dataflow:alpha".to_string(),
                label: "alpha".to_string(),
                file: Some(file_a),
            }],
            edges: vec![],
        },
        ..ProjectIr::default()
    };

    let node_ids = vec![
        format!("symbol:{}::alpha", file_b.display()),
        "dataflow:alpha".to_string(),
    ];
    let snippets = graph.context_snippets_for_nodes(&node_ids, 120);
    assert!(
        !snippets.is_empty(),
        "should emit snippets for matched node file paths"
    );

    let total_chars = snippets
        .iter()
        .map(|snippet: &ContextSnippet| snippet.snippet.chars().count())
        .sum::<usize>();
    assert!(
        total_chars <= 120,
        "combined snippet output should obey char budget"
    );
    assert!(
        snippets.iter().all(|snippet| !snippet.snippet.is_empty()),
        "each returned snippet should contain source text"
    );
}

#[test]
fn subgraph_for_nodes_keeps_only_selected_nodes_and_internal_edges() {
    let graph = ProjectIr {
        file_graph: Graph {
            nodes: vec![
                FileNode {
                    id: "file:a".to_string(),
                    path: "a.rs".into(),
                    language: "rust".to_string(),
                },
                FileNode {
                    id: "file:b".to_string(),
                    path: "b.rs".into(),
                    language: "rust".to_string(),
                },
            ],
            edges: vec![BasicEdge {
                from: "file:a".to_string(),
                to: "file:b".to_string(),
                relation: "depends_on".to_string(),
            }],
        },
        ..ProjectIr::default()
    };

    let fragment = graph.subgraph_for_nodes(&["file:a".to_string()]);
    assert_eq!(fragment.file_graph.nodes.len(), 1);
    assert_eq!(fragment.file_graph.nodes[0].id, "file:a");
    assert!(
        fragment.file_graph.edges.is_empty(),
        "edges crossing outside the selected node set should be excluded"
    );
}
