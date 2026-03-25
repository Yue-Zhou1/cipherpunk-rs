use std::collections::HashMap;
use std::fs;
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
use findings::json_export::to_findings_json;
use findings::sarif::to_sarif;
use jsonschema::{Draft, JSONSchema};

fn sample_manifest() -> AuditManifest {
    AuditManifest {
        audit_id: "audit-20260304-a1b2c3d4".to_string(),
        agent_version: "0.1.0".to_string(),
        source: ResolvedSource {
            local_path: PathBuf::from("/tmp/repo"),
            origin: SourceOrigin::Git {
                url: "https://github.com/example/repo".to_string(),
                original_ref: None,
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
        risk_score: 42,
        engines_run: vec!["crypto_zk".to_string()],
        engine_outcomes: vec![],
        coverage: None,
        optional_inputs_used: OptionalInputsSummary {
            spec_provided: true,
            prev_audit_provided: false,
            invariants_count: 0,
            entry_points_count: 1,
            llm_prose_used: false,
        },
    }
}

fn sample_findings() -> Vec<Finding> {
    vec![Finding {
        id: FindingId::new("F-CRYPTO-0001"),
        title: "Synthetic cryptographic misuse".to_string(),
        severity: Severity::High,
        category: FindingCategory::CryptoMisuse,
        framework: Framework::SP1,
        affected_components: vec![CodeLocation {
            crate_name: "crypto-app".to_string(),
            module: "lib".to_string(),
            file: PathBuf::from("crypto-app/src/lib.rs"),
            line_range: (10, 15),
            snippet: Some("aead_encrypt(key, nonce, msg);".to_string()),
        }],
        prerequisites: "attacker can submit crafted input".to_string(),
        exploit_path: "trigger deterministic nonce path".to_string(),
        impact: "confidentiality loss".to_string(),
        evidence: Evidence {
            command: Some("bash evidence-pack/F-CRYPTO-0001/reproduce.sh".to_string()),
            seed: None,
            trace_file: None,
            counterexample: None,
            harness_path: None,
            smt2_file: None,
            container_digest: "sha256:deadbeef".to_string(),
            tool_versions: HashMap::new(),
        },
        evidence_gate_level: 0,
        llm_generated: false,
        recommendation: "Use a domain-bound nonce derivation strategy".to_string(),
        regression_test: Some("test_nonce_uniqueness".to_string()),
        status: FindingStatus::Open,
        regression_check: false,
        verification_status: VerificationStatus::Verified,
    }]
}

#[test]
fn sarif_output_has_expected_shape() {
    let manifest = sample_manifest();
    let findings = sample_findings();
    let sarif = to_sarif(&findings, &manifest);

    assert_eq!(sarif.version, "2.1.0");
    assert_eq!(sarif.runs.len(), 1);
    assert_eq!(sarif.runs[0].results.len(), findings.len());
    assert_eq!(sarif.runs[0].results[0].rule_id, "F-CRYPTO-0001");
}

#[test]
fn sarif_output_validates_against_sarif_schema() {
    let manifest = sample_manifest();
    let findings = sample_findings();
    let sarif = to_sarif(&findings, &manifest);
    let sarif_json = serde_json::to_string_pretty(&sarif).expect("serialize sarif");
    let sarif_value: serde_json::Value =
        serde_json::from_str(&sarif_json).expect("parse sarif json");

    // Validate required SARIF 2.1.0 structure
    assert_eq!(sarif_value["version"], "2.1.0");
    assert_eq!(
        sarif_value["$schema"],
        "https://json.schemastore.org/sarif-2.1.0.json"
    );
    let runs = sarif_value["runs"].as_array().expect("runs array");
    assert!(!runs.is_empty());
    let run = &runs[0];
    assert!(run["tool"]["driver"]["name"].is_string());
    assert!(run["tool"]["driver"]["version"].is_string());
    let results = run["results"].as_array().expect("results array");
    for result in results {
        assert!(result["ruleId"].is_string(), "missing ruleId");
        assert!(result["level"].is_string(), "missing level");
        assert!(
            result["message"]["text"].is_string(),
            "missing message.text"
        );
        let locations = result["locations"].as_array().expect("locations array");
        for loc in locations {
            assert!(
                loc["physicalLocation"]["artifactLocation"]["uri"].is_string(),
                "missing artifact URI"
            );
            assert!(
                loc["physicalLocation"]["region"]["startLine"].is_number(),
                "missing startLine"
            );
            assert!(
                loc["physicalLocation"]["region"]["endLine"].is_number(),
                "missing endLine"
            );
        }
    }
}

#[test]
fn findings_json_validates_against_committed_finding_schema() {
    let findings = sample_findings();
    let findings_json = to_findings_json(&findings).expect("serialize findings");
    let instance: serde_json::Value = serde_json::from_str(&findings_json).expect("valid json");

    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("data/")
        .parent()
        .expect("crates/")
        .parent()
        .expect("repo root")
        .to_path_buf();
    let schema_text = fs::read_to_string(repo_root.join("docs/schemas/finding-schema.json"))
        .expect("read schema file");
    let schema: serde_json::Value = serde_json::from_str(&schema_text).expect("valid schema json");

    let validator = JSONSchema::options()
        .with_draft(Draft::Draft7)
        .compile(&schema)
        .expect("compile schema");
    if let Err(errors) = validator.validate(&instance) {
        let rendered = errors.map(|e| e.to_string()).collect::<Vec<_>>();
        panic!("schema validation failed: {}", rendered.join(", "));
    }
}
