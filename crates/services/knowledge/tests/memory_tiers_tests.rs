use std::collections::HashMap;
use std::path::PathBuf;

use audit_agent_core::finding::{
    CodeLocation, Evidence, Finding, FindingCategory, FindingId, FindingStatus, Framework,
    Severity, VerificationStatus,
};
use knowledge::{AuditMemoryEntry, FindingSeverityCounts, LongTermMemory, WorkingMemory};

#[test]
fn summarize_with_many_findings_is_bounded() {
    let mut working = WorkingMemory::new();

    for idx in 0..100 {
        working.record_finding(&sample_finding(
            idx,
            Severity::High,
            FindingCategory::CryptoMisuse,
            format!("critical crypto concern {idx}"),
        ));
    }

    let summary = working.summarize();
    assert!(
        summary.chars().count() <= 2_000,
        "summary exceeded 2,000 chars: {}",
        summary.chars().count()
    );
}

#[test]
fn adviser_context_includes_crypto_replay_and_race_findings() {
    let mut working = WorkingMemory::new();
    working.record_finding(&sample_finding(
        1,
        Severity::Critical,
        FindingCategory::CryptoMisuse,
        "nonce reuse in challenge stream".to_string(),
    ));
    working.record_finding(&sample_finding(
        2,
        Severity::High,
        FindingCategory::Replay,
        "replay window is effectively unbounded".to_string(),
    ));
    working.record_finding(&sample_finding(
        3,
        Severity::Medium,
        FindingCategory::Race,
        "race between settlement and cancellation".to_string(),
    ));
    working.record_finding(&sample_finding(
        4,
        Severity::Low,
        FindingCategory::SpecMismatch,
        "non-relevant category".to_string(),
    ));

    let context = working.context_for_role("adviser");
    assert!(context.contains("nonce reuse in challenge stream"));
    assert!(context.contains("replay window is effectively unbounded"));
    assert!(context.contains("race between settlement and cancellation"));
    assert!(!context.contains("non-relevant category"));
}

#[test]
fn unknown_role_returns_default_working_memory_message() {
    let working = WorkingMemory::new();
    assert_eq!(
        working.context_for_role("unknown-role"),
        "No additional working-memory context available."
    );
}

#[test]
fn long_term_memory_record_and_recall_roundtrip() {
    let mut memory = LongTermMemory::new();
    memory.record_audit_outcome(sample_memory_entry(
        "audit-1",
        vec!["halo2".to_string(), "crypto".to_string()],
    ));

    let recalled = memory.recall_similar(&["halo2".to_string()], 5);
    assert_eq!(recalled.len(), 1);
    assert_eq!(recalled[0].audit_id, "audit-1");
}

#[test]
fn long_term_memory_recall_without_match_is_empty() {
    let mut memory = LongTermMemory::new();
    memory.record_audit_outcome(sample_memory_entry(
        "audit-1",
        vec!["halo2".to_string(), "crypto".to_string()],
    ));

    let recalled = memory.recall_similar(&["solana".to_string()], 5);
    assert!(recalled.is_empty());
}

#[test]
fn long_term_memory_capacity_drops_oldest_entries() {
    let mut memory = LongTermMemory::new();
    for idx in 0..110 {
        memory.record_audit_outcome(sample_memory_entry(
            &format!("audit-{idx}"),
            vec!["crypto".to_string()],
        ));
    }

    let entries = memory.entries();
    assert_eq!(entries.len(), 100);
    assert_eq!(entries.first().expect("first entry").audit_id, "audit-10");
    assert_eq!(entries.last().expect("last entry").audit_id, "audit-109");
}

#[test]
fn long_term_memory_persist_and_reload_from_path() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut memory = LongTermMemory::load_from_path(temp.path()).expect("load memory");
    memory.record_audit_outcome(sample_memory_entry(
        "audit-persist",
        vec!["halo2".to_string(), "crypto".to_string()],
    ));
    memory.persist().expect("persist memory");

    let reloaded = LongTermMemory::load_from_path(temp.path()).expect("reload memory");
    assert_eq!(reloaded.entries().len(), 1);
    assert_eq!(reloaded.entries()[0].audit_id, "audit-persist");
}

fn sample_finding(
    idx: usize,
    severity: Severity,
    category: FindingCategory,
    title: String,
) -> Finding {
    Finding {
        id: FindingId::new(format!("F-{idx}")),
        title,
        severity,
        category,
        framework: Framework::Static,
        affected_components: vec![CodeLocation {
            crate_name: "demo".to_string(),
            module: "mod".to_string(),
            file: PathBuf::from("src/lib.rs"),
            line_range: (10, 20),
            snippet: None,
        }],
        prerequisites: "none".to_string(),
        exploit_path: "none".to_string(),
        impact: "high".to_string(),
        evidence: Evidence {
            command: None,
            seed: None,
            trace_file: None,
            counterexample: None,
            harness_path: None,
            smt2_file: None,
            container_digest: "n/a".to_string(),
            tool_versions: HashMap::new(),
        },
        evidence_gate_level: 0,
        llm_generated: false,
        recommendation: "fix".to_string(),
        regression_test: None,
        status: FindingStatus::Open,
        regression_check: false,
        verification_status: VerificationStatus::unverified("test"),
    }
}

fn sample_memory_entry(audit_id: &str, tags: Vec<String>) -> AuditMemoryEntry {
    AuditMemoryEntry {
        audit_id: audit_id.to_string(),
        timestamp: "2026-03-22T00:00:00Z".to_string(),
        source_description: "local workspace".to_string(),
        findings_by_severity: FindingSeverityCounts {
            critical: 1,
            high: 2,
            medium: 3,
            low: 4,
            observation: 5,
        },
        engines_used: vec!["crypto-zk".to_string()],
        key_findings: vec!["nonce reuse".to_string()],
        tags,
    }
}
