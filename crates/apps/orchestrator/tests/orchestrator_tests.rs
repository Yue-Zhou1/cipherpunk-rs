use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use audit_agent_core::audit_config::{
    AuditConfig, BudgetConfig, EngineConfig, LlmConfig, OptionalInputs, ParsedPreviousAudit,
    PriorFinding, PriorFindingStatus, ResolvedScope, ResolvedSource, SourceOrigin,
};
use audit_agent_core::engine::{AuditContext, AuditEngine};
use audit_agent_core::finding::{
    CodeLocation, Evidence, Finding, FindingCategory, FindingId, FindingStatus, Framework,
    Severity, VerificationStatus,
};
use audit_agent_core::output::{EngineOutcome, EngineStatus};
use orchestrator::{AuditEvent, AuditEventSink, AuditOrchestrator};
use tempfile::tempdir;

fn write_workspace(root: &Path) {
    std::fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"rollup-core\"]\nresolver = \"2\"\n",
    )
    .expect("write workspace");
    std::fs::create_dir_all(root.join("rollup-core/src")).expect("mkdir");
    std::fs::write(
        root.join("rollup-core/Cargo.toml"),
        "[package]\nname = \"rollup-core\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("write crate manifest");
    std::fs::write(
        root.join("rollup-core/src/lib.rs"),
        "pub fn process() { helper(); }\nfn helper() {}\n",
    )
    .expect("write source");
}

fn sample_config(workspace_root: &Path, output_dir: &Path) -> AuditConfig {
    AuditConfig {
        audit_id: "audit-orchestrator-test".to_string(),
        source: ResolvedSource {
            local_path: workspace_root.to_path_buf(),
            origin: SourceOrigin::Local {
                original_path: workspace_root.to_path_buf(),
            },
            commit_hash: "abcdef1234567890abcdef1234567890abcdef12".to_string(),
            content_hash: "sha256:content".to_string(),
        },
        scope: ResolvedScope {
            target_crates: vec!["rollup-core".to_string()],
            excluded_crates: vec![],
            build_matrix: vec![],
            detected_frameworks: vec![Framework::SP1],
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
            semantic_index_timeout_secs: 60,
        },
        optional_inputs: OptionalInputs {
            spec_document: None,
            previous_audit: None,
            custom_invariants: vec![],
            known_entry_points: vec![],
        },
        llm: LlmConfig {
            api_key_present: false,
            provider: None,
            no_llm_prose: true,
            roles: std::collections::HashMap::new(),
        },
        output_dir: output_dir.to_path_buf(),
    }
}

fn sample_finding() -> Finding {
    Finding {
        id: FindingId::new("F-CRYPTO-777"),
        title: "Duplicate finding".to_string(),
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
        prerequisites: String::new(),
        exploit_path: String::new(),
        impact: "impact".to_string(),
        evidence: Evidence {
            command: None,
            seed: None,
            trace_file: None,
            counterexample: None,
            harness_path: None,
            smt2_file: None,
            container_digest: "sha256:test".to_string(),
            tool_versions: HashMap::new(),
        },
        evidence_gate_level: 0,
        llm_generated: false,
        recommendation: "fix".to_string(),
        regression_test: None,
        status: FindingStatus::Open,
        regression_check: false,
        verification_status: VerificationStatus::Verified,
    }
}

#[derive(Default)]
struct RecordingSink {
    events: Mutex<Vec<AuditEvent>>,
}

impl AuditEventSink for RecordingSink {
    fn emit(&self, event: AuditEvent) {
        self.events.lock().expect("event lock").push(event);
    }
}

struct StaticEngine {
    findings: Vec<Finding>,
}

#[async_trait]
impl AuditEngine for StaticEngine {
    fn name(&self) -> &str {
        "static-engine"
    }

    async fn analyze(&self, _ctx: &AuditContext) -> anyhow::Result<Vec<Finding>> {
        Ok(self.findings.clone())
    }

    async fn supports(&self, _ctx: &AuditContext) -> bool {
        true
    }
}

struct FailingEngine;

#[async_trait]
impl AuditEngine for FailingEngine {
    fn name(&self) -> &str {
        "failing-engine"
    }

    async fn analyze(&self, _ctx: &AuditContext) -> anyhow::Result<Vec<Finding>> {
        anyhow::bail!("simulated failure")
    }

    async fn supports(&self, _ctx: &AuditContext) -> bool {
        true
    }
}

#[tokio::test]
async fn produce_outputs_deduplicates_marks_regression_and_writes_reports() {
    let dir = tempdir().expect("tempdir");
    write_workspace(dir.path());
    let output_dir = dir.path().join("audit-output");
    let evidence_zip = dir.path().join("evidence-pack.zip");
    std::fs::write(&evidence_zip, "zip").expect("write evidence zip");

    let mut config = sample_config(dir.path(), &output_dir);
    config.optional_inputs.previous_audit = Some(ParsedPreviousAudit {
        source_path: PathBuf::from("/tmp/prev.md"),
        prior_findings: vec![PriorFinding {
            id: "F-CRYPTO-777".to_string(),
            title: "Duplicate finding".to_string(),
            severity: Severity::High,
            description: "old finding".to_string(),
            status: PriorFindingStatus::Reported,
            location_hint: Some("rollup-core/src/lib.rs:10".to_string()),
        }],
    });

    let mut cached = sample_finding();
    cached
        .evidence
        .tool_versions
        .insert("analysis_origin".to_string(), "cache".to_string());
    let mut fresh = sample_finding();
    fresh
        .evidence
        .tool_versions
        .insert("analysis_origin".to_string(), "new".to_string());

    let orchestrator = AuditOrchestrator::new(output_dir.clone(), evidence_zip);
    let outputs = orchestrator
        .produce_outputs(
            &[cached, fresh],
            &[EngineOutcome {
                engine: "static-engine".to_string(),
                status: EngineStatus::Completed,
                findings_count: 1,
                duration_ms: 10,
            }],
            &config,
        )
        .await
        .expect("produce outputs");

    assert_eq!(outputs.findings.len(), 1, "dedup should remove duplicates");
    assert!(outputs.findings[0].regression_check);
    assert!(output_dir.join("report-executive.md").exists());
    assert!(output_dir.join("report-technical.md").exists());
    assert!(output_dir.join("audit-manifest.json").exists());
}

#[tokio::test]
async fn produce_outputs_emits_audit_completed_event() {
    let dir = tempdir().expect("tempdir");
    write_workspace(dir.path());
    let output_dir = dir.path().join("audit-output");
    let evidence_zip = dir.path().join("evidence-pack.zip");
    std::fs::write(&evidence_zip, "zip").expect("write evidence zip");

    let sink = Arc::new(RecordingSink::default());
    let sink_trait: Arc<dyn AuditEventSink> = sink.clone();
    let orchestrator =
        AuditOrchestrator::new(output_dir.clone(), evidence_zip).with_event_sink(sink_trait);
    let config = sample_config(dir.path(), &output_dir);
    orchestrator
        .produce_outputs(
            &[sample_finding()],
            &[EngineOutcome {
                engine: "static-engine".to_string(),
                status: EngineStatus::Completed,
                findings_count: 1,
                duration_ms: 10,
            }],
            &config,
        )
        .await
        .expect("produce outputs");

    let events = sink.events.lock().expect("event lock");
    assert_eq!(events.len(), 1);
    assert!(matches!(
        &events[0],
        AuditEvent::AuditCompleted { audit_id, .. } if audit_id == "audit-orchestrator-test"
    ));
}

#[tokio::test]
async fn run_executes_engines_and_returns_outputs() {
    let dir = tempdir().expect("tempdir");
    write_workspace(dir.path());
    let output_dir = dir.path().join("audit-output");
    let evidence_zip = dir.path().join("evidence-pack.zip");
    std::fs::write(&evidence_zip, "zip").expect("write evidence zip");

    let orchestrator =
        AuditOrchestrator::new(output_dir.clone(), evidence_zip).with_engines(vec![Box::new(
            StaticEngine {
                findings: vec![sample_finding(), sample_finding()],
            },
        )]);
    let config = sample_config(dir.path(), &output_dir);
    let outputs = orchestrator.run(&config).await.expect("run orchestrator");

    assert_eq!(outputs.findings.len(), 1);
    assert_eq!(outputs.manifest.audit_id, "audit-orchestrator-test");
    assert_eq!(outputs.manifest.engine_outcomes.len(), 1);
    assert!(outputs.manifest.coverage.is_some());
}

#[tokio::test]
async fn execute_dag_continues_after_engine_failure_and_records_outcomes() {
    let dir = tempdir().expect("tempdir");
    write_workspace(dir.path());
    let output_dir = dir.path().join("audit-output");
    let evidence_zip = dir.path().join("evidence-pack.zip");
    std::fs::write(&evidence_zip, "zip").expect("write evidence zip");

    let orchestrator = AuditOrchestrator::new(output_dir.clone(), evidence_zip).with_engines(vec![
        Box::new(FailingEngine),
        Box::new(StaticEngine {
            findings: vec![sample_finding()],
        }),
    ]);
    let config = sample_config(dir.path(), &output_dir);
    let dag = orchestrator.build_dag(&config);
    let (findings, outcomes) = orchestrator
        .execute_dag(&dag, &config)
        .await
        .expect("execute dag");

    assert_eq!(findings.len(), 1, "healthy engines should still contribute");
    assert_eq!(outcomes.len(), 2);
    assert!(outcomes.iter().any(|outcome| {
        outcome.engine == "failing-engine" && matches!(outcome.status, EngineStatus::Failed { .. })
    }));
    assert!(outcomes.iter().any(|outcome| {
        outcome.engine == "static-engine" && matches!(outcome.status, EngineStatus::Completed)
    }));
}

#[tokio::test]
async fn run_emits_engine_lifecycle_events_for_success_and_failure() {
    let dir = tempdir().expect("tempdir");
    write_workspace(dir.path());
    let output_dir = dir.path().join("audit-output");
    let evidence_zip = dir.path().join("evidence-pack.zip");
    std::fs::write(&evidence_zip, "zip").expect("write evidence zip");

    let sink = Arc::new(RecordingSink::default());
    let sink_trait: Arc<dyn AuditEventSink> = sink.clone();
    let orchestrator = AuditOrchestrator::new(output_dir.clone(), evidence_zip)
        .with_event_sink(sink_trait)
        .with_engines(vec![
            Box::new(FailingEngine),
            Box::new(StaticEngine {
                findings: vec![sample_finding()],
            }),
        ]);
    let config = sample_config(dir.path(), &output_dir);
    orchestrator.run(&config).await.expect("run orchestrator");

    let events = sink.events.lock().expect("event lock");
    assert!(events.iter().any(|event| {
        matches!(
            event,
            AuditEvent::EngineFailed { engine, .. } if engine == "failing-engine"
        )
    }));
    assert!(events.iter().any(|event| {
        matches!(
            event,
            AuditEvent::EngineCompleted { engine, .. } if engine == "static-engine"
        )
    }));
    assert!(
        events
            .iter()
            .any(|event| { matches!(event, AuditEvent::AuditCompleted { .. }) })
    );
}

#[tokio::test]
async fn produce_outputs_aggregates_manifest_tool_versions_and_container_digests() {
    let dir = tempdir().expect("tempdir");
    write_workspace(dir.path());
    let output_dir = dir.path().join("audit-output");
    let evidence_zip = dir.path().join("evidence-pack.zip");
    std::fs::write(&evidence_zip, "zip").expect("write evidence zip");

    let mut finding = sample_finding();
    finding.id = FindingId::new("F-CRYPTO-999");
    finding
        .evidence
        .tool_versions
        .insert("semantic_backend".to_string(), "rust-analyzer".to_string());
    finding
        .evidence
        .tool_versions
        .insert("z3".to_string(), "4.13.0".to_string());
    finding.evidence.container_digest = "sha256:deadbeef".to_string();

    let orchestrator = AuditOrchestrator::new(output_dir.clone(), evidence_zip);
    let config = sample_config(dir.path(), &output_dir);
    let outputs = orchestrator
        .produce_outputs(
            &[finding],
            &[EngineOutcome {
                engine: "static-engine".to_string(),
                status: EngineStatus::Completed,
                findings_count: 1,
                duration_ms: 10,
            }],
            &config,
        )
        .await
        .expect("produce outputs");

    assert_eq!(
        outputs.manifest.tool_versions.get("semantic_backend"),
        Some(&"rust-analyzer".to_string())
    );
    assert_eq!(
        outputs.manifest.tool_versions.get("z3"),
        Some(&"4.13.0".to_string())
    );
    assert_eq!(
        outputs.manifest.container_digests.get("F-CRYPTO-999"),
        Some(&"sha256:deadbeef".to_string())
    );
}

#[tokio::test]
async fn produce_outputs_records_engine_outcomes_and_partial_coverage() {
    let dir = tempdir().expect("tempdir");
    write_workspace(dir.path());
    let output_dir = dir.path().join("audit-output");
    let evidence_zip = dir.path().join("evidence-pack.zip");
    std::fs::write(&evidence_zip, "zip").expect("write evidence zip");

    let orchestrator = AuditOrchestrator::new(output_dir.clone(), evidence_zip);
    let config = sample_config(dir.path(), &output_dir);
    let outcomes = vec![
        EngineOutcome {
            engine: "static-engine".to_string(),
            status: EngineStatus::Completed,
            findings_count: 1,
            duration_ms: 10,
        },
        EngineOutcome {
            engine: "failing-engine".to_string(),
            status: EngineStatus::Failed {
                reason: "simulated".to_string(),
            },
            findings_count: 0,
            duration_ms: 3,
        },
    ];

    let outputs = orchestrator
        .produce_outputs(&[sample_finding()], &outcomes, &config)
        .await
        .expect("produce outputs");

    assert_eq!(outputs.manifest.engine_outcomes, outcomes);
    let coverage = outputs
        .manifest
        .coverage
        .as_ref()
        .expect("coverage should be present");
    assert!(!coverage.coverage_complete);
    assert_eq!(coverage.engines_failed, 1);
    assert_eq!(
        outputs.manifest.engines_run,
        vec!["static-engine".to_string(), "failing-engine".to_string()]
    );
}
