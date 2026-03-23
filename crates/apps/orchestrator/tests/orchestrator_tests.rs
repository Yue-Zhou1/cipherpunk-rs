use std::collections::HashMap;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow};
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
use llm::{
    AdviserService, CompletionOpts, LlmProvider as ServiceLlmProvider, ProviderFailoverRecord,
};
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

struct BudgetSensitiveEngine {
    calls: Arc<Mutex<u32>>,
    min_timeout_secs: u64,
    findings: Vec<Finding>,
}

#[async_trait]
impl AuditEngine for BudgetSensitiveEngine {
    fn name(&self) -> &str {
        "z3-budget-sensitive"
    }

    async fn analyze(&self, ctx: &AuditContext) -> anyhow::Result<Vec<Finding>> {
        *self.calls.lock().expect("calls lock") += 1;
        if ctx.config.budget.z3_timeout_secs < self.min_timeout_secs {
            anyhow::bail!(
                "timeout: z3 budget {} below required {}",
                ctx.config.budget.z3_timeout_secs,
                self.min_timeout_secs
            );
        }
        Ok(self.findings.clone())
    }

    async fn supports(&self, _ctx: &AuditContext) -> bool {
        true
    }
}

struct AlwaysFailEngine {
    name: String,
    reason: String,
    calls: Arc<Mutex<u32>>,
}

#[async_trait]
impl AuditEngine for AlwaysFailEngine {
    fn name(&self) -> &str {
        &self.name
    }

    async fn analyze(&self, _ctx: &AuditContext) -> anyhow::Result<Vec<Finding>> {
        *self.calls.lock().expect("calls lock") += 1;
        anyhow::bail!("{}", self.reason);
    }

    async fn supports(&self, _ctx: &AuditContext) -> bool {
        true
    }
}

#[derive(Debug)]
struct SequenceAdviserProvider {
    responses: Mutex<VecDeque<Result<String>>>,
    calls: Arc<Mutex<u32>>,
}

impl SequenceAdviserProvider {
    fn new(responses: Vec<Result<String>>, calls: Arc<Mutex<u32>>) -> Self {
        Self {
            responses: Mutex::new(responses.into()),
            calls,
        }
    }
}

#[async_trait]
impl ServiceLlmProvider for SequenceAdviserProvider {
    async fn complete(&self, _prompt: &str, _opts: &CompletionOpts) -> Result<String> {
        *self.calls.lock().expect("calls lock") += 1;
        self.responses
            .lock()
            .expect("responses lock")
            .pop_front()
            .unwrap_or_else(|| Err(anyhow!("missing adviser response")))
    }

    fn name(&self) -> &str {
        "adviser-sequence"
    }

    fn is_available(&self) -> bool {
        true
    }

    fn model(&self) -> Option<&str> {
        Some("adviser-sequence-v1")
    }
}

#[derive(Debug)]
struct PromptCapturingAdviserProvider {
    prompt: Arc<Mutex<Option<String>>>,
    response: String,
}

#[async_trait]
impl ServiceLlmProvider for PromptCapturingAdviserProvider {
    async fn complete(&self, prompt: &str, _opts: &CompletionOpts) -> Result<String> {
        *self.prompt.lock().expect("prompt lock") = Some(prompt.to_string());
        Ok(self.response.clone())
    }

    fn name(&self) -> &str {
        "adviser-prompt-capture"
    }

    fn is_available(&self) -> bool {
        true
    }

    fn model(&self) -> Option<&str> {
        Some("adviser-prompt-capture-v1")
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
                adviser_suggestion: None,
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
                adviser_suggestion: None,
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
                adviser_suggestion: None,
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
            adviser_suggestion: None,
        },
        EngineOutcome {
            engine: "failing-engine".to_string(),
            status: EngineStatus::Failed {
                reason: "simulated".to_string(),
            },
            findings_count: 0,
            duration_ms: 3,
            adviser_suggestion: None,
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

#[tokio::test]
async fn produce_outputs_includes_failover_warnings_and_emits_failover_events() {
    let dir = tempdir().expect("tempdir");
    write_workspace(dir.path());
    let output_dir = dir.path().join("audit-output");
    let evidence_zip = dir.path().join("evidence-pack.zip");
    std::fs::write(&evidence_zip, "zip").expect("write evidence zip");

    let sink = Arc::new(RecordingSink::default());
    let sink_trait: Arc<dyn AuditEventSink> = sink.clone();
    let failover_events = Arc::new(Mutex::new(vec![ProviderFailoverRecord {
        from: "openai".to_string(),
        to: "template-fallback".to_string(),
        role: "Scaffolding".to_string(),
        reason: "transient error".to_string(),
    }]));
    let orchestrator = AuditOrchestrator::new(output_dir.clone(), evidence_zip)
        .with_event_sink(sink_trait)
        .with_failover_events(Arc::clone(&failover_events));
    let config = sample_config(dir.path(), &output_dir);
    let outcomes = vec![EngineOutcome {
        engine: "static-engine".to_string(),
        status: EngineStatus::Completed,
        findings_count: 1,
        duration_ms: 10,
        adviser_suggestion: None,
    }];

    let outputs = orchestrator
        .produce_outputs(&[sample_finding()], &outcomes, &config)
        .await
        .expect("produce outputs");

    let coverage = outputs
        .manifest
        .coverage
        .as_ref()
        .expect("coverage should be present");
    assert_eq!(coverage.failover_warnings.len(), 1);
    assert!(
        coverage
            .warnings
            .iter()
            .any(|warning| warning.contains("provider failover occurred")),
        "coverage warnings should include failover warning"
    );

    let events = sink.events.lock().expect("event lock");
    assert!(events.iter().any(|event| {
        matches!(
            event,
            AuditEvent::ProviderFailover { from, to, .. }
                if from == "openai" && to == "template-fallback"
        )
    }));
}

#[tokio::test]
async fn execute_dag_applies_adviser_retry_with_relaxed_budget() {
    let dir = tempdir().expect("tempdir");
    write_workspace(dir.path());
    let output_dir = dir.path().join("audit-output");
    let evidence_zip = dir.path().join("evidence-pack.zip");
    std::fs::write(&evidence_zip, "zip").expect("write evidence zip");

    let calls = Arc::new(Mutex::new(0_u32));
    let engine_calls = Arc::new(Mutex::new(0_u32));
    let engine = BudgetSensitiveEngine {
        calls: Arc::clone(&engine_calls),
        min_timeout_secs: 300,
        findings: vec![sample_finding()],
    };

    let adviser_json = r#"{"action":{"type":"RetryWithRelaxedBudget","timeout_secs":600,"memory_mb":2048},"rationale":"z3 timeout likely due budget"}"#;
    let adviser_provider =
        SequenceAdviserProvider::new(vec![Ok(adviser_json.to_string())], Arc::clone(&calls));
    let adviser = AdviserService::new(Arc::new(adviser_provider));

    let sink = Arc::new(RecordingSink::default());
    let sink_trait: Arc<dyn AuditEventSink> = sink.clone();
    let orchestrator = AuditOrchestrator::new(output_dir.clone(), evidence_zip)
        .with_event_sink(sink_trait)
        .with_adviser(adviser)
        .with_engines(vec![Box::new(engine)]);
    let mut config = sample_config(dir.path(), &output_dir);
    config.budget.z3_timeout_secs = 60;

    let dag = orchestrator.build_dag(&config);
    let (findings, outcomes) = orchestrator
        .execute_dag(&dag, &config)
        .await
        .expect("execute dag");

    assert_eq!(findings.len(), 1, "retry should recover findings");
    assert_eq!(*engine_calls.lock().expect("calls"), 2);
    assert_eq!(outcomes.len(), 1);
    assert!(matches!(outcomes[0].status, EngineStatus::Completed));
    assert_eq!(*calls.lock().expect("adviser calls"), 1);

    let events = sink.events.lock().expect("event lock");
    assert!(events.iter().any(|event| {
        matches!(
            event,
            AuditEvent::AdviserConsulted { applied, .. } if *applied
        )
    }));
}

#[tokio::test]
async fn execute_dag_records_skip_suggestion_but_marks_engine_failed() {
    let dir = tempdir().expect("tempdir");
    write_workspace(dir.path());
    let output_dir = dir.path().join("audit-output");
    let evidence_zip = dir.path().join("evidence-pack.zip");
    std::fs::write(&evidence_zip, "zip").expect("write evidence zip");

    let calls = Arc::new(Mutex::new(0_u32));
    let engine = AlwaysFailEngine {
        name: "z3-skip-test".to_string(),
        reason: "timeout while checking".to_string(),
        calls: Arc::new(Mutex::new(0_u32)),
    };
    let adviser_json = r#"{"action":{"type":"SkipEngine","reason":"likely incompatible"},"rationale":"input unsupported for this engine"}"#;
    let adviser_provider =
        SequenceAdviserProvider::new(vec![Ok(adviser_json.to_string())], Arc::clone(&calls));
    let adviser = AdviserService::new(Arc::new(adviser_provider));

    let orchestrator = AuditOrchestrator::new(output_dir.clone(), evidence_zip)
        .with_adviser(adviser)
        .with_engines(vec![Box::new(engine)]);
    let config = sample_config(dir.path(), &output_dir);
    let dag = orchestrator.build_dag(&config);
    let (_findings, outcomes) = orchestrator
        .execute_dag(&dag, &config)
        .await
        .expect("execute dag");

    assert_eq!(outcomes.len(), 1);
    assert!(
        matches!(outcomes[0].status, EngineStatus::Failed { .. }),
        "skip suggestion must not convert failure into skipped status"
    );
    assert!(
        outcomes[0]
            .adviser_suggestion
            .as_deref()
            .unwrap_or_default()
            .contains("SkipEngine"),
        "skip suggestion should be persisted for review"
    );
    assert_eq!(*calls.lock().expect("adviser calls"), 1);
}

#[tokio::test]
async fn execute_dag_does_not_mark_retry_applied_when_budget_is_unchanged() {
    let dir = tempdir().expect("tempdir");
    write_workspace(dir.path());
    let output_dir = dir.path().join("audit-output");
    let evidence_zip = dir.path().join("evidence-pack.zip");
    std::fs::write(&evidence_zip, "zip").expect("write evidence zip");

    let adviser_calls = Arc::new(Mutex::new(0_u32));
    let retry_json = r#"{"action":{"type":"RetryWithRelaxedBudget","timeout_secs":600,"memory_mb":8192},"rationale":"increase memory only"}"#;
    let adviser_provider =
        SequenceAdviserProvider::new(vec![Ok(retry_json.to_string())], Arc::clone(&adviser_calls));
    let adviser = AdviserService::new(Arc::new(adviser_provider));

    let engine_calls = Arc::new(Mutex::new(0_u32));
    let engine = AlwaysFailEngine {
        name: "z3-noop-retry".to_string(),
        reason: "timeout while solving".to_string(),
        calls: Arc::clone(&engine_calls),
    };

    let sink = Arc::new(RecordingSink::default());
    let sink_trait: Arc<dyn AuditEventSink> = sink.clone();
    let orchestrator = AuditOrchestrator::new(output_dir.clone(), evidence_zip)
        .with_event_sink(sink_trait)
        .with_adviser(adviser)
        .with_engines(vec![Box::new(engine)]);
    let config = sample_config(dir.path(), &output_dir);
    let dag = orchestrator.build_dag(&config);
    let (_findings, outcomes) = orchestrator
        .execute_dag(&dag, &config)
        .await
        .expect("execute dag");

    assert_eq!(outcomes.len(), 1);
    assert!(matches!(outcomes[0].status, EngineStatus::Failed { .. }));
    assert_eq!(
        *engine_calls.lock().expect("engine calls"),
        1,
        "no effective budget change should not trigger retry"
    );
    assert_eq!(*adviser_calls.lock().expect("adviser calls"), 1);

    let events = sink.events.lock().expect("event lock");
    assert!(events.iter().any(|event| {
        matches!(
            event,
            AuditEvent::AdviserConsulted { applied, .. } if !applied
        )
    }));
}

#[tokio::test]
async fn execute_dag_redacts_engine_name_in_adviser_prompt_context() {
    let dir = tempdir().expect("tempdir");
    write_workspace(dir.path());
    let output_dir = dir.path().join("audit-output");
    let evidence_zip = dir.path().join("evidence-pack.zip");
    std::fs::write(&evidence_zip, "zip").expect("write evidence zip");

    let captured_prompt = Arc::new(Mutex::new(None::<String>));
    let adviser_response =
        r#"{"action":{"type":"NoSuggestion"},"rationale":"no deterministic recovery available"}"#;
    let adviser_provider = PromptCapturingAdviserProvider {
        prompt: Arc::clone(&captured_prompt),
        response: adviser_response.to_string(),
    };
    let adviser = AdviserService::new(Arc::new(adviser_provider));

    let engine = AlwaysFailEngine {
        name: "z3-internal-budget-sensitive-v2".to_string(),
        reason: "timeout while solving".to_string(),
        calls: Arc::new(Mutex::new(0_u32)),
    };

    let orchestrator = AuditOrchestrator::new(output_dir.clone(), evidence_zip)
        .with_adviser(adviser)
        .with_engines(vec![Box::new(engine)]);
    let config = sample_config(dir.path(), &output_dir);
    let dag = orchestrator.build_dag(&config);
    let (_findings, _outcomes) = orchestrator
        .execute_dag(&dag, &config)
        .await
        .expect("execute dag");

    let prompt = captured_prompt
        .lock()
        .expect("prompt lock")
        .clone()
        .expect("prompt should be captured");
    assert!(prompt.contains("Engine: smt"));
    assert!(!prompt.contains("z3-internal-budget-sensitive-v2"));
}

#[tokio::test]
async fn execute_dag_enforces_adviser_call_limit() {
    let dir = tempdir().expect("tempdir");
    write_workspace(dir.path());
    let output_dir = dir.path().join("audit-output");
    let evidence_zip = dir.path().join("evidence-pack.zip");
    std::fs::write(&evidence_zip, "zip").expect("write evidence zip");

    let adviser_calls = Arc::new(Mutex::new(0_u32));
    let no_suggestion_json =
        r#"{"action":{"type":"NoSuggestion"},"rationale":"no deterministic recovery available"}"#;
    let adviser_provider = SequenceAdviserProvider::new(
        (0..8)
            .map(|_| Ok(no_suggestion_json.to_string()))
            .collect::<Vec<_>>(),
        Arc::clone(&adviser_calls),
    );
    let adviser = AdviserService::new(Arc::new(adviser_provider));

    let mut engines: Vec<Box<dyn AuditEngine>> = Vec::new();
    for idx in 0..6 {
        engines.push(Box::new(AlwaysFailEngine {
            name: format!("z3-limit-{idx}"),
            reason: "timeout while solving".to_string(),
            calls: Arc::new(Mutex::new(0_u32)),
        }));
    }

    let orchestrator = AuditOrchestrator::new(output_dir.clone(), evidence_zip)
        .with_adviser(adviser)
        .with_engines(engines);
    let config = sample_config(dir.path(), &output_dir);
    let dag = orchestrator.build_dag(&config);
    let (_findings, outcomes) = orchestrator
        .execute_dag(&dag, &config)
        .await
        .expect("execute dag");

    assert_eq!(outcomes.len(), 6);
    assert_eq!(*adviser_calls.lock().expect("adviser calls"), 5);
}

#[tokio::test]
async fn execute_dag_enforces_single_adviser_retry_per_engine() {
    let dir = tempdir().expect("tempdir");
    write_workspace(dir.path());
    let output_dir = dir.path().join("audit-output");
    let evidence_zip = dir.path().join("evidence-pack.zip");
    std::fs::write(&evidence_zip, "zip").expect("write evidence zip");

    let adviser_calls = Arc::new(Mutex::new(0_u32));
    let retry_json = r#"{"action":{"type":"RetryWithRelaxedBudget","timeout_secs":900,"memory_mb":4096},"rationale":"retry once with larger budget"}"#;
    let adviser_provider = SequenceAdviserProvider::new(
        vec![Ok(retry_json.to_string()), Ok(retry_json.to_string())],
        Arc::clone(&adviser_calls),
    );
    let adviser = AdviserService::new(Arc::new(adviser_provider));

    let engine_calls = Arc::new(Mutex::new(0_u32));
    let engine = AlwaysFailEngine {
        name: "z3-retry-limit".to_string(),
        reason: "timeout while solving".to_string(),
        calls: Arc::clone(&engine_calls),
    };

    let orchestrator = AuditOrchestrator::new(output_dir.clone(), evidence_zip)
        .with_adviser(adviser)
        .with_engines(vec![Box::new(engine)]);
    let config = sample_config(dir.path(), &output_dir);
    let dag = orchestrator.build_dag(&config);
    let (_findings, outcomes) = orchestrator
        .execute_dag(&dag, &config)
        .await
        .expect("execute dag");

    assert_eq!(outcomes.len(), 1);
    assert!(matches!(outcomes[0].status, EngineStatus::Failed { .. }));
    assert_eq!(
        *engine_calls.lock().expect("engine calls"),
        2,
        "engine should run at most initial attempt + one adviser retry"
    );
    assert!(
        *adviser_calls.lock().expect("adviser calls") >= 1,
        "adviser should be consulted on first failure"
    );
}
