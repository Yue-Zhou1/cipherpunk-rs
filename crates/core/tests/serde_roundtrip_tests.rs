use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use audit_agent_core::audit_config::{
    AuditConfig, BudgetConfig, BuildVariant, CustomAssertionTarget, CustomInvariant, EngineConfig,
    EntryPoint, ExtractionMethod, OptionalInputs, OptionalInputsSummary, ParsedPreviousAudit,
    ParsedSpecDocument, PriorFinding, PriorFindingStatus, ResolvedScope, ResolvedSource,
    SourceOrigin, SpecSection, StructuredConstraint,
};
use audit_agent_core::engine::AuditContext;
use audit_agent_core::finding::{
    CodeLocation, Evidence, Finding, FindingCategory, FindingId, FindingStatus, Framework,
    Severity, VerificationStatus,
};
use audit_agent_core::output::{AuditManifest, FindingCounts};
use audit_agent_core::workspace::CargoWorkspace;
use audit_agent_core::{EvidenceStore, SandboxExecutor};
use chrono::Utc;
use serde::{Serialize, de::DeserializeOwned};

fn round_trip<T>(value: T)
where
    T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug,
{
    let json = serde_json::to_string(&value).expect("serialize");
    let decoded: T = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(decoded, value);
}

fn sample_resolved_source() -> ResolvedSource {
    ResolvedSource {
        local_path: PathBuf::from("/tmp/workspace"),
        origin: SourceOrigin::Git {
            url: "https://github.com/example/repo".to_string(),
            original_ref: Some("main".to_string()),
        },
        commit_hash: "1234567890123456789012345678901234567890".to_string(),
        content_hash: "abcd".to_string(),
    }
}

fn sample_optional_inputs() -> OptionalInputs {
    OptionalInputs {
        spec_document: Some(ParsedSpecDocument {
            source_path: PathBuf::from("spec.md"),
            extracted_constraints: vec![audit_agent_core::audit_config::CandidateConstraint {
                structured: StructuredConstraint::Custom {
                    assertion_code: "x < 10".to_string(),
                    target: CustomAssertionTarget::Rust,
                },
                source_text: "x must be less than 10".to_string(),
                source_section: "sec".to_string(),
                confidence: audit_agent_core::audit_config::Confidence::Medium,
                extraction_method: ExtractionMethod::PatternMatch,
            }],
            sections: vec![SpecSection {
                title: "Overview".to_string(),
                content: "hello".to_string(),
            }],
            raw_text: "raw".to_string(),
        }),
        previous_audit: Some(ParsedPreviousAudit {
            source_path: PathBuf::from("prev.md"),
            prior_findings: vec![PriorFinding {
                id: "A-1".to_string(),
                title: "title".to_string(),
                severity: Severity::Low,
                description: "desc".to_string(),
                status: PriorFindingStatus::Reported,
                location_hint: Some("src/lib.rs:10".to_string()),
            }],
        }),
        custom_invariants: vec![CustomInvariant {
            id: "INV-1".to_string(),
            name: "Name".to_string(),
            description: "Desc".to_string(),
            check_expr: "true".to_string(),
            violation_severity: Severity::High,
            spec_ref: Some("spec".to_string()),
        }],
        known_entry_points: vec![EntryPoint {
            crate_name: "crate".to_string(),
            function: "module::f".to_string(),
        }],
    }
}

#[test]
fn finding_round_trip() {
    let finding = Finding {
        id: FindingId::new("F-ZK-0042"),
        title: "Field element deserialization without canonicality check".to_string(),
        severity: Severity::High,
        category: FindingCategory::CryptoMisuse,
        framework: Framework::Halo2,
        affected_components: vec![CodeLocation {
            crate_name: "zk-prover-core".to_string(),
            module: "decoder".to_string(),
            file: PathBuf::from("src/decoder.rs"),
            line_range: (10, 20),
            snippet: Some("fn decode() {}".to_string()),
        }],
        prerequisites: "attacker controls input".to_string(),
        exploit_path: "send malformed point".to_string(),
        impact: "proof forgery risk".to_string(),
        evidence: Evidence {
            command: Some("cargo kani".to_string()),
            seed: Some("42".to_string()),
            trace_file: Some(PathBuf::from("trace.json")),
            counterexample: Some("x=0".to_string()),
            harness_path: Some(PathBuf::from("harness.rs")),
            smt2_file: Some(PathBuf::from("query.smt2")),
            container_digest: "sha256:1234".to_string(),
            tool_versions: HashMap::from([
                ("kani".to_string(), "0.57.0".to_string()),
                ("z3".to_string(), "4.13.0".to_string()),
            ]),
        },
        evidence_gate_level: 3,
        llm_generated: false,
        recommendation: "Add canonicality check".to_string(),
        regression_test: Some("assert!(true);".to_string()),
        status: FindingStatus::Open,
        regression_check: false,
        verification_status: VerificationStatus::Verified,
    };

    round_trip(finding);
}

#[test]
fn audit_config_and_manifest_round_trip() {
    let source = sample_resolved_source();

    let scope = ResolvedScope {
        target_crates: vec!["core".to_string()],
        excluded_crates: vec!["bench".to_string()],
        build_matrix: vec![BuildVariant {
            features: vec!["asm".to_string()],
            target_triple: "x86_64-unknown-linux-gnu".to_string(),
            label: "asm".to_string(),
        }],
        detected_frameworks: vec![Framework::SP1],
    };

    let config = AuditConfig {
        audit_id: "audit-20260101-1234".to_string(),
        source: source.clone(),
        scope: scope.clone(),
        engines: EngineConfig {
            crypto_zk: true,
            distributed: false,
        },
        budget: BudgetConfig {
            kani_timeout_secs: 300,
            z3_timeout_secs: 600,
            fuzz_duration_secs: 3600,
            madsim_ticks: 100_000,
            max_llm_retries: 3,
            semantic_index_timeout_secs: 120,
        },
        optional_inputs: sample_optional_inputs(),
        llm: audit_agent_core::audit_config::LlmConfig {
            api_key_present: false,
            provider: None,
            no_llm_prose: false,
        },
        output_dir: PathBuf::from("audit-output"),
    };

    round_trip(config.clone());

    let manifest = AuditManifest {
        audit_id: config.audit_id,
        agent_version: "0.1.0".to_string(),
        source,
        started_at: Utc::now(),
        completed_at: Some(Utc::now()),
        scope,
        tool_versions: HashMap::from([("kani".to_string(), "0.57.0".to_string())]),
        container_digests: HashMap::from([("kani".to_string(), "sha256:abcd".to_string())]),
        finding_counts: FindingCounts::default(),
        risk_score: 100,
        engines_run: vec!["crypto_zk".to_string()],
        optional_inputs_used: OptionalInputsSummary {
            spec_provided: true,
            prev_audit_provided: false,
            invariants_count: 1,
            entry_points_count: 1,
            llm_prose_used: false,
        },
    };

    round_trip(manifest);
}

#[test]
fn audit_context_is_constructible_with_all_fields() {
    let config = Arc::new(AuditConfig {
        audit_id: "audit-20260101-1234".to_string(),
        source: sample_resolved_source(),
        scope: ResolvedScope {
            target_crates: vec!["core".to_string()],
            excluded_crates: vec![],
            build_matrix: vec![],
            detected_frameworks: vec![],
        },
        engines: EngineConfig {
            crypto_zk: true,
            distributed: false,
        },
        budget: BudgetConfig {
            kani_timeout_secs: 300,
            z3_timeout_secs: 600,
            fuzz_duration_secs: 3600,
            madsim_ticks: 100_000,
            max_llm_retries: 3,
            semantic_index_timeout_secs: 120,
        },
        optional_inputs: sample_optional_inputs(),
        llm: audit_agent_core::audit_config::LlmConfig {
            api_key_present: false,
            provider: None,
            no_llm_prose: true,
        },
        output_dir: PathBuf::from("audit-output"),
    });

    let context = AuditContext {
        config,
        workspace: Arc::new(CargoWorkspace::default()),
        sandbox: Arc::new(SandboxExecutor::default()),
        evidence_store: Arc::new(EvidenceStore::default()),
        llm: None,
    };

    assert!(!context.config.audit_id.is_empty());
}
