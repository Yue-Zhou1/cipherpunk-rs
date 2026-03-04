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
use chrono::Utc;
use report::technical::render_technical_report;

fn sample_manifest() -> AuditManifest {
    AuditManifest {
        audit_id: "audit-20260304-a1b2c3d4".to_string(),
        agent_version: "0.1.0".to_string(),
        source: ResolvedSource {
            local_path: PathBuf::from("/tmp/repo"),
            origin: SourceOrigin::Local {
                original_path: PathBuf::from("/tmp/repo"),
            },
            commit_hash: "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2".to_string(),
            content_hash: "sha256:content".to_string(),
        },
        started_at: Utc::now(),
        completed_at: None,
        scope: ResolvedScope {
            target_crates: vec!["crypto-app".to_string()],
            excluded_crates: vec![],
            build_matrix: vec![],
            detected_frameworks: vec![Framework::SP1],
        },
        tool_versions: HashMap::new(),
        container_digests: HashMap::new(),
        finding_counts: FindingCounts::default(),
        risk_score: 55,
        engines_run: vec!["crypto_zk".to_string()],
        optional_inputs_used: OptionalInputsSummary {
            spec_provided: true,
            prev_audit_provided: false,
            invariants_count: 0,
            entry_points_count: 1,
            llm_prose_used: false,
        },
    }
}

fn sample_finding() -> Finding {
    Finding {
        id: FindingId::new("F-CRYPTO-0002"),
        title: "Missing domain separator".to_string(),
        severity: Severity::High,
        category: FindingCategory::CryptoMisuse,
        framework: Framework::SP1,
        affected_components: vec![CodeLocation {
            crate_name: "crypto-app".to_string(),
            module: "lib".to_string(),
            file: PathBuf::from("crypto-app/src/lib.rs"),
            line_range: (20, 24),
            snippet: Some("transcript_hash_no_domain(transcript);".to_string()),
        }],
        prerequisites: "attacker controls transcript context".to_string(),
        exploit_path: "cross-protocol transcript collision".to_string(),
        impact: "proof context confusion".to_string(),
        evidence: Evidence {
            command: Some("bash evidence-pack/F-CRYPTO-0002/reproduce.sh".to_string()),
            seed: None,
            trace_file: None,
            counterexample: None,
            harness_path: None,
            smt2_file: None,
            container_digest: "sha256:feedface".to_string(),
            tool_versions: HashMap::new(),
        },
        evidence_gate_level: 0,
        llm_generated: false,
        recommendation: "bind transcript with domain tag".to_string(),
        regression_test: Some("test_domain_separator_required".to_string()),
        status: FindingStatus::Open,
        regression_check: false,
        verification_status: VerificationStatus::Verified,
    }
}

#[test]
fn technical_report_renders_snippets_and_inline_reproduce_commands() {
    let report = render_technical_report(&[sample_finding()], &sample_manifest());
    assert!(report.contains("# Technical Audit Report"));
    assert!(report.contains("```rust"));
    assert!(report.contains("transcript_hash_no_domain"));
    assert!(report.contains("Reproduce: `bash evidence-pack/F-CRYPTO-0002/reproduce.sh`"));
    assert_eq!(report.matches("```").count() % 2, 0, "broken markdown fences");
}
