use std::path::PathBuf;

use project_ir::{FeatureNode, FileNode, ProjectIr};

#[test]
fn rust_without_crypto_indicators_falls_back_to_core_audit() {
    let mut ir = ProjectIr::default();
    ir.file_graph.nodes.push(FileNode {
        id: "f1".to_string(),
        path: PathBuf::from("src/state_machine.rs"),
        language: "rust".to_string(),
    });
    ir.feature_graph.nodes.push(FeatureNode {
        id: "feat-1".to_string(),
        name: "state transition validator".to_string(),
        source: "src/state_machine.rs".to_string(),
    });

    let plan = ir.checklist_plan();
    assert_eq!(plan.domains.len(), 1);
    assert_eq!(plan.domains[0].id, "core-audit");
}

#[test]
fn rust_with_crypto_indicators_selects_crypto_domain() {
    let mut ir = ProjectIr::default();
    ir.file_graph.nodes.push(FileNode {
        id: "f1".to_string(),
        path: PathBuf::from("src/crypto/signature.rs"),
        language: "rust".to_string(),
    });
    ir.feature_graph.nodes.push(FeatureNode {
        id: "feat-1".to_string(),
        name: "verify signature".to_string(),
        source: "src/crypto/signature.rs".to_string(),
    });

    let plan = ir.checklist_plan();
    assert!(plan.domains.iter().any(|domain| domain.id == "crypto"));
}
