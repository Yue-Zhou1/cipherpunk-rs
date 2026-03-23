use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use audit_agent_core::audit_config::{AuditConfig, ParsedPreviousAudit};
use audit_agent_core::engine::{AuditContext, AuditEngine, EvidenceWriter, SandboxRunner};
use audit_agent_core::finding::Finding;
use audit_agent_core::output::{
    AuditManifest, AuditOutputs, CoverageReport, EngineOutcome, EngineStatus, FindingCounts,
};
use audit_agent_core::session::AuditSession;
use audit_agent_core::tooling::ToolActionStatus;
use chrono::Utc;
use findings::pipeline::{
    deduplicate_findings, mark_regression_checks as mark_regression_checks_by_key,
};
use intake::diff::AnalysisCache;
use intake::summarize_optional_inputs;
use intake::workspace::WorkspaceAnalyzer;
use llm::LlmProvider;
use report::generator::{ReportGenerator, ReportGeneratorOptions};
use session_store::{SessionEvent, SessionStore};

pub mod events;
pub mod jobs;
mod runtime;
pub mod tool_actions;

pub use audit_agent_core::tooling::{ToolActionRequest, ToolActionResult, ToolFamily};
pub use events::{AuditEvent, AuditEventSink, JobLifecycleEvent};
pub use jobs::{AuditJob, AuditJobKind, AuditJobStatus};
pub use runtime::OrchestratorRuntime;
pub use tool_actions::plan_tool_action;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditDag {
    pub nodes: Vec<String>,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct FindingsDb;

impl FindingsDb {
    pub fn deduplicate(&self, findings: &[Finding]) -> Vec<Finding> {
        deduplicate_findings(findings)
    }

    pub fn mark_regression_checks(
        &self,
        findings: &mut [Finding],
        previous_audit: Option<&ParsedPreviousAudit>,
    ) {
        if let Some(previous_audit) = previous_audit {
            mark_regression_checks_by_key(findings, previous_audit);
        }
    }
}

pub struct AuditOrchestrator {
    pub engines: Vec<Box<dyn AuditEngine>>,
    pub runtime: OrchestratorRuntime,
    pub findings_db: FindingsDb,
    pub cache: Arc<AnalysisCache>,
    pub session_store: Option<Arc<SessionStore>>,
    pub output_dir: PathBuf,
    pub evidence_pack_zip: PathBuf,
    pub llm: Option<Arc<dyn LlmProvider>>,
    pub event_sink: Option<Arc<dyn AuditEventSink>>,
}

impl AuditOrchestrator {
    pub fn new(output_dir: PathBuf, evidence_pack_zip: PathBuf) -> Self {
        Self {
            engines: vec![],
            runtime: OrchestratorRuntime::default(),
            findings_db: FindingsDb,
            cache: Arc::new(AnalysisCache::default()),
            session_store: None,
            output_dir,
            evidence_pack_zip,
            llm: None,
            event_sink: None,
        }
    }

    pub fn for_tests() -> Self {
        let mut orchestrator = Self::new(
            std::env::temp_dir().join("audit-agent-orchestrator-tests"),
            std::env::temp_dir().join("audit-agent-orchestrator-tests-evidence.zip"),
        );
        orchestrator.runtime = OrchestratorRuntime::for_tests();
        orchestrator
    }

    pub fn with_engines(mut self, engines: Vec<Box<dyn AuditEngine>>) -> Self {
        self.engines = engines;
        self
    }

    pub fn with_runtime(mut self, runtime: OrchestratorRuntime) -> Self {
        self.runtime = runtime;
        self
    }

    pub fn with_sandbox(mut self, sandbox: Arc<dyn SandboxRunner>) -> Self {
        self.runtime.sandbox = sandbox;
        self
    }

    pub fn with_evidence_writer(mut self, evidence_writer: Arc<dyn EvidenceWriter>) -> Self {
        self.runtime.evidence_writer = evidence_writer;
        self
    }

    pub fn with_context_llm(mut self, llm: Arc<dyn audit_agent_core::LlmProvider>) -> Self {
        self.runtime.context_llm = Some(llm);
        self
    }

    pub fn with_session_store(mut self, store: Arc<SessionStore>) -> Self {
        self.session_store = Some(store);
        self
    }

    pub fn with_llm(mut self, llm: Arc<dyn LlmProvider>) -> Self {
        self.llm = Some(llm);
        self
    }

    pub fn with_event_sink(mut self, event_sink: Arc<dyn AuditEventSink>) -> Self {
        self.event_sink = Some(event_sink);
        self
    }

    pub async fn bootstrap_jobs(&self, session: &AuditSession) -> Result<Vec<AuditJob>> {
        let mut jobs = Vec::<AuditJob>::new();
        jobs.push(AuditJob::queued(
            &session.session_id,
            AuditJobKind::BuildProjectIr,
            jobs.len(),
        ));
        jobs.push(AuditJob::queued(
            &session.session_id,
            AuditJobKind::GenerateAiOverview,
            jobs.len(),
        ));
        jobs.push(AuditJob::queued(
            &session.session_id,
            AuditJobKind::PlanChecklists,
            jobs.len(),
        ));
        for engine in &self.engines {
            jobs.push(AuditJob::queued(
                &session.session_id,
                AuditJobKind::RunEngine {
                    engine_name: engine.name().to_string(),
                },
                jobs.len(),
            ));
        }
        for domain_id in &session.selected_domains {
            jobs.push(AuditJob::queued(
                &session.session_id,
                AuditJobKind::RunDomainChecklist {
                    domain_id: domain_id.clone(),
                },
                jobs.len(),
            ));
        }
        jobs.push(AuditJob::queued(
            &session.session_id,
            AuditJobKind::ExportReports,
            jobs.len(),
        ));

        self.persist_bootstrap_events(session, &jobs)?;
        Ok(jobs)
    }

    pub fn test_context(&self) -> AuditContext {
        AuditContext {
            config: Arc::new(runtime::build_test_config()),
            workspace: Arc::new(runtime::build_test_workspace()),
            sandbox: Arc::clone(&self.runtime.sandbox),
            evidence_store: Arc::clone(&self.runtime.evidence_writer),
            llm: self.runtime.context_llm.clone(),
        }
    }

    pub async fn run_tool_action(&self, request: ToolActionRequest) -> Result<ToolActionResult> {
        let artifact_root = self
            .output_dir
            .join("tool-runs")
            .join(request.session_id.clone());

        if request.tool_family == ToolFamily::LeanExternal {
            let base_url = std::env::var("AXLE_API_URL")
                .unwrap_or_else(|_| engine_lean::types::AXLE_BASE_URL.to_string());
            let _ = std::fs::create_dir_all(&artifact_root);
            return engine_lean::execute_lean_action(&request, &base_url, &artifact_root).await;
        }

        let plan = tool_actions::plan_tool_action(&request);
        let workspace_root = request
            .workspace_root
            .clone()
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."));
        let _ = std::fs::create_dir_all(&artifact_root);
        let sandbox_request =
            tool_actions::sandbox_request(&plan, &request.budget, &workspace_root, &artifact_root);
        let sandbox_result = self.runtime.sandbox.execute(sandbox_request).await?;
        let status = if sandbox_result.exit_code == 0 {
            ToolActionStatus::Completed
        } else {
            ToolActionStatus::Failed
        };

        let mut artifact_refs = plan.artifact_refs.clone();
        artifact_refs.extend(
            sandbox_result
                .artifacts
                .iter()
                .map(|path| path.to_string_lossy().to_string()),
        );

        let result = ToolActionResult {
            action_id: format!("action-{}", Utc::now().timestamp_micros()),
            session_id: request.session_id.clone(),
            tool_family: plan.tool_family,
            target: request.target,
            command: plan.command,
            artifact_refs,
            rationale: plan.rationale,
            status,
            stdout_preview: optional_preview(&sandbox_result.stdout),
            stderr_preview: optional_preview(&sandbox_result.stderr),
        };

        if let Some(store) = &self.session_store {
            let payload = serde_json::to_string(&result)?;
            let event = SessionEvent {
                event_id: format!("tool-action:{}", result.action_id),
                event_type: "tool.action".to_string(),
                payload,
                created_at: Utc::now(),
            };
            store.append_event(&result.session_id, &event)?;
        }

        Ok(result)
    }

    pub async fn run(&self, config: &AuditConfig) -> Result<AuditOutputs> {
        let dag = self.build_dag(config);
        let (findings, outcomes) = self.execute_dag(&dag, config).await?;
        self.produce_outputs(&findings, &outcomes, config).await
    }

    pub fn build_dag(&self, _config: &AuditConfig) -> AuditDag {
        let mut nodes = self
            .engines
            .iter()
            .map(|engine| format!("engine:{}", engine.name()))
            .collect::<Vec<_>>();
        nodes.push("produce_outputs".to_string());
        AuditDag { nodes }
    }

    pub async fn execute_dag(
        &self,
        _dag: &AuditDag,
        config: &AuditConfig,
    ) -> Result<(Vec<Finding>, Vec<EngineOutcome>)> {
        let workspace = Arc::new(WorkspaceAnalyzer::analyze(&config.source.local_path)?);
        let ctx = AuditContext {
            config: Arc::new(config.clone()),
            workspace,
            sandbox: Arc::clone(&self.runtime.sandbox),
            evidence_store: Arc::clone(&self.runtime.evidence_writer),
            llm: self.runtime.context_llm.clone(),
        };

        let mut findings = Vec::<Finding>::new();
        let mut outcomes = Vec::<EngineOutcome>::new();
        for engine in &self.engines {
            let engine_name = engine.name().to_string();
            let started_at = std::time::Instant::now();

            if !engine.supports(&ctx).await {
                outcomes.push(EngineOutcome {
                    engine: engine_name,
                    status: EngineStatus::Skipped {
                        reason: "engine reported unsupported for this context".to_string(),
                    },
                    findings_count: 0,
                    duration_ms: started_at.elapsed().as_millis() as u64,
                });
                continue;
            }

            match engine.analyze(&ctx).await {
                Ok(engine_findings) => {
                    let count = engine_findings.len();
                    let duration_ms = started_at.elapsed().as_millis() as u64;
                    findings.extend(engine_findings);

                    if let Some(sink) = &self.event_sink {
                        sink.emit(AuditEvent::EngineCompleted {
                            engine: engine_name.clone(),
                            findings_count: count,
                            duration_ms,
                        });
                    }

                    outcomes.push(EngineOutcome {
                        engine: engine_name,
                        status: EngineStatus::Completed,
                        findings_count: count,
                        duration_ms,
                    });
                }
                Err(err) => {
                    let duration_ms = started_at.elapsed().as_millis() as u64;

                    if let Some(sink) = &self.event_sink {
                        sink.emit(AuditEvent::EngineFailed {
                            engine: engine_name.clone(),
                            reason: err.to_string(),
                        });
                    }

                    outcomes.push(EngineOutcome {
                        engine: engine_name,
                        status: EngineStatus::Failed {
                            reason: err.to_string(),
                        },
                        findings_count: 0,
                        duration_ms,
                    });
                }
            }
        }
        Ok((findings, outcomes))
    }

    pub async fn produce_outputs(
        &self,
        findings: &[Finding],
        outcomes: &[EngineOutcome],
        config: &AuditConfig,
    ) -> Result<AuditOutputs> {
        let mut deduplicated = self.findings_db.deduplicate(findings);
        self.findings_db.mark_regression_checks(
            &mut deduplicated,
            config.optional_inputs.previous_audit.as_ref(),
        );

        let finding_counts = FindingCounts::from(&deduplicated);
        let coverage = CoverageReport::from_outcomes(outcomes);
        let engine_outcomes = outcomes.to_vec();
        let engines_run = outcomes
            .iter()
            .map(|outcome| outcome.engine.clone())
            .collect::<Vec<_>>();
        let (tool_versions, container_digests) = aggregate_manifest_metadata(&deduplicated);
        let now = Utc::now();
        let manifest = AuditManifest {
            audit_id: config.audit_id.clone(),
            agent_version: env!("CARGO_PKG_VERSION").to_string(),
            source: config.source.clone(),
            started_at: now,
            completed_at: Some(now),
            scope: config.scope.clone(),
            tool_versions: tool_versions.clone(),
            container_digests: container_digests.clone(),
            finding_counts: finding_counts.clone(),
            risk_score: finding_counts.risk_score(),
            engines_run: engines_run.clone(),
            engine_outcomes: engine_outcomes.clone(),
            coverage: Some(coverage.clone()),
            optional_inputs_used: summarize_optional_inputs(config),
        };

        let generator = ReportGenerator::new(
            deduplicated.clone(),
            manifest,
            ReportGeneratorOptions {
                llm: self.llm.clone(),
                no_llm_prose: config.llm.no_llm_prose,
                evidence_pack_zip: self.evidence_pack_zip.clone(),
                previous_audit: config.optional_inputs.previous_audit.clone(),
            },
        );
        generator.generate_all(&self.output_dir).await?;

        let manifest = read_written_manifest(&self.output_dir).unwrap_or_else(|_| AuditManifest {
            audit_id: config.audit_id.clone(),
            agent_version: env!("CARGO_PKG_VERSION").to_string(),
            source: config.source.clone(),
            started_at: now,
            completed_at: Some(now),
            scope: config.scope.clone(),
            tool_versions,
            container_digests,
            finding_counts: finding_counts.clone(),
            risk_score: finding_counts.risk_score(),
            engines_run,
            engine_outcomes,
            coverage: Some(coverage),
            optional_inputs_used: summarize_optional_inputs(config),
        });

        if let Some(sink) = &self.event_sink {
            sink.emit(AuditEvent::AuditCompleted {
                audit_id: manifest.audit_id.clone(),
                output_dir: self.output_dir.clone(),
                finding_count: deduplicated.len(),
            });
        }

        Ok(AuditOutputs {
            dir: self.output_dir.clone(),
            manifest,
            findings: deduplicated,
            candidates: vec![],
            review_notes: vec![],
        })
    }

    fn persist_bootstrap_events(&self, session: &AuditSession, jobs: &[AuditJob]) -> Result<()> {
        let Some(store) = &self.session_store else {
            return Ok(());
        };

        for job in jobs {
            let lifecycle = JobLifecycleEvent::queued(job)?;
            store.append_event(&session.session_id, &lifecycle.to_session_event())?;
        }

        Ok(())
    }
}

fn read_written_manifest(output_dir: &Path) -> Result<AuditManifest> {
    let path = output_dir.join("audit-manifest.json");
    let content = std::fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&content)?)
}

fn aggregate_manifest_metadata(
    findings: &[Finding],
) -> (HashMap<String, String>, HashMap<String, String>) {
    let mut tool_versions = HashMap::<String, String>::new();
    let mut container_digests = HashMap::<String, String>::new();

    for finding in findings {
        for (key, value) in &finding.evidence.tool_versions {
            // First-writer-wins policy: keep the first observed tool version for each key.
            // This preserves deterministic manifest metadata across repeated findings.
            tool_versions
                .entry(key.clone())
                .or_insert_with(|| value.clone());
        }

        let digest = finding.evidence.container_digest.trim();
        if digest.is_empty() || digest.eq_ignore_ascii_case("n/a") {
            continue;
        }

        container_digests.insert(finding.id.to_string(), digest.to_string());
    }

    (tool_versions, container_digests)
}

fn optional_preview(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    let limit = 1_024usize;
    let mut chars = trimmed.chars();
    let preview = chars.by_ref().take(limit).collect::<String>();
    if chars.next().is_none() {
        return Some(preview);
    }

    Some(format!("{preview}..."))
}
