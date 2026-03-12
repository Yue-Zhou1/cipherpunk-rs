use knowledge::models::{AdjudicatedCase, ReproPattern, ToolSequence};
use knowledge::KnowledgeBase;

#[test]
fn playbooks_load_and_route_tools_for_rust_crypto() {
    let kb = KnowledgeBase::load_from_repo_root().expect("load knowledge base");
    let routing = kb.route_tools(&["rust".to_string(), "crypto".to_string()]);
    assert!(routing.iter().any(|tool| tool == "kani"));
    assert!(routing.iter().any(|tool| tool == "cargo-fuzz"));
}

#[test]
fn domains_include_required_checklist_items() {
    let kb = KnowledgeBase::load_from_repo_root().expect("load knowledge base");
    let domain = kb.domain("zk").expect("zk domain exists");
    assert!(domain.items.iter().any(|item| item.id == "witness-shape"));
}

#[test]
fn adjudicated_case_store_supports_ingest_and_retrieval() {
    let mut kb = KnowledgeBase::load_from_repo_root().expect("load knowledge base");

    kb.ingest_true_positive(AdjudicatedCase {
        id: "TP-1".to_string(),
        title: "nonce reuse".to_string(),
        summary: "true positive from crypto review".to_string(),
        tags: vec!["crypto".to_string()],
    });
    kb.ingest_false_positive(AdjudicatedCase {
        id: "FP-1".to_string(),
        title: "expected warning".to_string(),
        summary: "intentional behavior".to_string(),
        tags: vec!["consensus".to_string()],
    });
    kb.ingest_tool_sequence(ToolSequence {
        id: "SEQ-1".to_string(),
        tools: vec!["kani".to_string(), "z3".to_string()],
        rationale: "prove and validate edge case".to_string(),
    });
    kb.ingest_repro_pattern(ReproPattern {
        id: "RP-1".to_string(),
        title: "minimal replay harness".to_string(),
        steps: vec!["build".to_string(), "replay".to_string()],
    });

    assert_eq!(kb.true_positives().len(), 1);
    assert_eq!(kb.false_positives().len(), 1);
    assert_eq!(kb.tool_sequences().len(), 1);
    assert_eq!(kb.repro_patterns().len(), 1);
}
