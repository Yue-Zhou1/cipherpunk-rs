use std::collections::HashMap;
use std::path::PathBuf;

use audit_agent_core::audit_config::{
    OptionalInputsSummary, ResolvedScope, ResolvedSource, SourceOrigin,
};
use audit_agent_core::finding::{
    CodeLocation, Evidence, Finding, FindingCategory, FindingId, FindingStatus, Framework,
    Severity, VerificationStatus,
};
use audit_agent_core::output::{AuditManifest, FindingCounts};
use audit_agent_core::session::AuditRecord;
use chrono::Utc;
use report::generator::{V3ChecklistCoverage, V3ReportBundle, V3ToolInventory, render_v3_report};
use report::typst::render_technical_typst;

#[test]
fn technical_report_includes_verified_findings_candidate_appendix_and_tool_inventory() {
    let report = render_v3_report(sample_v3_bundle());
    assert!(report.contains("Verified Findings"));
    assert!(report.contains("Unverified Candidates"));
    assert!(report.contains("Tool Inventory"));
    assert!(report.contains("Checklist Coverage"));
}

#[test]
fn typst_render_uses_templates_and_replaces_placeholders() {
    let rendered = render_technical_typst(&sample_v3_bundle());
    assert!(rendered.contains("= Technical Audit Report"));
    assert!(rendered.contains("= Unverified Candidates Appendix"));
    assert!(!rendered.contains("<tool_inventory>"));
    assert!(!rendered.contains("<checklist_coverage>"));
    assert!(!rendered.contains("<verified_findings>"));
    assert!(!rendered.contains("<candidates>"));
}

fn sample_v3_bundle() -> V3ReportBundle {
    let manifest = AuditManifest {
        audit_id: "audit-v3-test".to_string(),
        agent_version: "0.1.0".to_string(),
        source: ResolvedSource {
            local_path: PathBuf::from("/tmp/repo"),
            origin: SourceOrigin::Local {
                original_path: PathBuf::from("/tmp/repo"),
            },
            commit_hash: "abc123".to_string(),
            content_hash: "sha256:content".to_string(),
        },
        started_at: Utc::now(),
        completed_at: Some(Utc::now()),
        scope: ResolvedScope {
            target_crates: vec!["rollup-core".to_string()],
            excluded_crates: vec![],
            build_matrix: vec![],
            detected_frameworks: vec![Framework::SP1],
        },
        tool_versions: HashMap::new(),
        container_digests: HashMap::new(),
        finding_counts: FindingCounts::default(),
        risk_score: 66,
        engines_run: vec!["crypto_zk".to_string()],
        optional_inputs_used: OptionalInputsSummary {
            spec_provided: false,
            prev_audit_provided: false,
            invariants_count: 0,
            entry_points_count: 0,
            llm_prose_used: false,
        },
    };

    let finding = Finding {
        id: FindingId::new("F-CRYPTO-0100"),
        title: "Sample verified finding".to_string(),
        severity: Severity::High,
        category: FindingCategory::CryptoMisuse,
        framework: Framework::SP1,
        affected_components: vec![CodeLocation {
            crate_name: "rollup-core".to_string(),
            module: "lib".to_string(),
            file: PathBuf::from("rollup-core/src/lib.rs"),
            line_range: (10, 12),
            snippet: Some("unsafe_hash(bytes);".to_string()),
        }],
        prerequisites: "attacker controls bytes".to_string(),
        exploit_path: "hash collision".to_string(),
        impact: "integrity risk".to_string(),
        evidence: Evidence {
            command: Some("bash evidence-pack/F-CRYPTO-0100/reproduce.sh".to_string()),
            seed: None,
            trace_file: None,
            counterexample: None,
            harness_path: None,
            smt2_file: None,
            container_digest: "sha256:deadbeef".to_string(),
            tool_versions: HashMap::new(),
        },
        evidence_gate_level: 2,
        llm_generated: false,
        recommendation: "replace unsafe hash primitive".to_string(),
        regression_test: Some("test_safe_hash_required".to_string()),
        status: FindingStatus::Open,
        regression_check: false,
        verification_status: VerificationStatus::Verified,
    };

    let candidate = AuditRecord::candidate(
        "cand-1",
        "Potential replay issue in transaction queue",
        VerificationStatus::unverified("Requires analyst confirmation"),
    );

    V3ReportBundle {
        manifest,
        findings: vec![finding],
        candidates: vec![candidate],
        tool_inventory: vec![V3ToolInventory {
            tool: "Kani".to_string(),
            version: "0.57.0".to_string(),
            container_digest: "sha256:kani".to_string(),
        }],
        checklist_coverage: vec![V3ChecklistCoverage {
            domain: "crypto".to_string(),
            status: "completed".to_string(),
            notes: "Signer and verifier paths reviewed".to_string(),
        }],
        recommended_fixes: vec!["Enforce domain separation in transcript hashing".to_string()],
        regression_plan: vec!["Add regression test for replay-safe signer path".to_string()],
    }
}
