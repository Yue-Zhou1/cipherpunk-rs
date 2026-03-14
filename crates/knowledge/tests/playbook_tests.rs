use knowledge::KnowledgeBase;
use knowledge::models::{AdjudicatedCase, ReproPattern, ToolSequence};

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

#[test]
fn adjudicated_case_store_persists_across_reload() {
    let temp = tempfile::tempdir().expect("temp dir");
    let store_path = temp.path().join("adjudicated-feedback.yaml");

    let mut kb = KnowledgeBase::load_from_repo_root_with_store(&store_path)
        .expect("load knowledge base with feedback store");
    kb.ingest_true_positive(AdjudicatedCase {
        id: "TP-persist".to_string(),
        title: "persisted case".to_string(),
        summary: "should survive reload".to_string(),
        tags: vec!["crypto".to_string()],
    });
    kb.persist_feedback_store(&store_path)
        .expect("persist feedback store");

    let reloaded = KnowledgeBase::load_from_repo_root_with_store(&store_path)
        .expect("reload knowledge base with feedback store");
    assert!(
        reloaded
            .true_positives()
            .iter()
            .any(|case| case.id == "TP-persist")
    );
}

#[test]
fn feedback_store_overwrite_is_clean_and_readable() {
    let temp = tempfile::tempdir().expect("temp dir");
    let store_path = temp.path().join("adjudicated-feedback.yaml");

    let mut kb = KnowledgeBase::load_from_repo_root_with_store(&store_path)
        .expect("load knowledge base with feedback store");
    kb.ingest_true_positive(AdjudicatedCase {
        id: "TP-first".to_string(),
        title: "first".to_string(),
        summary: "first write".to_string(),
        tags: vec!["crypto".to_string()],
    });
    kb.persist_feedback_store(&store_path)
        .expect("persist first store");

    let mut kb = KnowledgeBase::load_from_repo_root_with_store(&store_path)
        .expect("reload knowledge base with feedback store");
    kb.ingest_true_positive(AdjudicatedCase {
        id: "TP-second".to_string(),
        title: "second".to_string(),
        summary: "second write".to_string(),
        tags: vec!["zk".to_string()],
    });
    kb.persist_feedback_store(&store_path)
        .expect("persist second store");

    let reloaded =
        KnowledgeBase::load_from_repo_root_with_store(&store_path).expect("reload second store");
    assert!(
        reloaded
            .true_positives()
            .iter()
            .any(|case| case.id == "TP-second")
    );
    assert!(
        !temp
            .path()
            .read_dir()
            .expect("list temp dir")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .any(|name| name.contains(".tmp-")),
        "temporary feedback-store files should not be left behind"
    );
}
