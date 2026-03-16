use std::collections::HashMap;
use std::path::PathBuf;

use audit_agent_core::audit_config::{ParsedPreviousAudit, PriorFinding, PriorFindingStatus};
use audit_agent_core::finding::{
    CodeLocation, Evidence, Finding, FindingCategory, FindingId, FindingStatus, Framework,
    Severity, VerificationStatus,
};
use findings::pipeline::{deduplicate_findings, mark_regression_checks};

fn sample_finding(id: &str, file: &str, line: u32, analysis_origin: &str) -> Finding {
    let mut tool_versions = HashMap::new();
    tool_versions.insert("analysis_origin".to_string(), analysis_origin.to_string());

    Finding {
        id: FindingId::new(id.to_string()),
        title: "sample finding".to_string(),
        severity: Severity::Medium,
        category: FindingCategory::CryptoMisuse,
        framework: Framework::SP1,
        affected_components: vec![CodeLocation {
            crate_name: "rollup-core".to_string(),
            module: "lib".to_string(),
            file: PathBuf::from(file),
            line_range: (line, line + 1),
            snippet: Some("unsafe_call()".to_string()),
        }],
        prerequisites: String::new(),
        exploit_path: String::new(),
        impact: format!("impact-{analysis_origin}"),
        evidence: Evidence {
            command: None,
            seed: None,
            trace_file: None,
            counterexample: None,
            harness_path: None,
            smt2_file: None,
            container_digest: "sha256:test".to_string(),
            tool_versions,
        },
        evidence_gate_level: 0,
        llm_generated: false,
        recommendation: String::new(),
        regression_test: None,
        status: FindingStatus::Open,
        regression_check: false,
        verification_status: VerificationStatus::Verified,
    }
}

#[test]
fn deduplication_uses_rule_file_and_start_line_key() {
    let cached = sample_finding("F-CRYPTO-1", "rollup-core/src/lib.rs", 10, "cache");
    let fresh = sample_finding("F-CRYPTO-1", "rollup-core/src/lib.rs", 10, "new");
    let distinct = sample_finding("F-CRYPTO-1", "rollup-core/src/lib.rs", 11, "new");

    let deduped = deduplicate_findings(&[cached, fresh, distinct]);
    assert_eq!(deduped.len(), 2);
    assert!(deduped.iter().any(|finding| finding.impact == "impact-new"
        && finding.affected_components[0].line_range.0 == 10));
}

#[test]
fn regression_checks_match_dedup_key_with_location_hint() {
    let mut findings = vec![
        sample_finding("F-CRYPTO-1", "rollup-core/src/lib.rs", 10, "new"),
        sample_finding("F-CRYPTO-1", "rollup-core/src/lib.rs", 20, "new"),
    ];
    let previous = ParsedPreviousAudit {
        source_path: PathBuf::from("/tmp/prev.md"),
        prior_findings: vec![PriorFinding {
            id: "F-CRYPTO-1".to_string(),
            title: "sample finding".to_string(),
            severity: Severity::Medium,
            description: "old".to_string(),
            status: PriorFindingStatus::Reported,
            location_hint: Some("rollup-core/src/lib.rs:20".to_string()),
        }],
    };

    mark_regression_checks(&mut findings, &previous);
    assert!(!findings[0].regression_check);
    assert!(findings[1].regression_check);
}
