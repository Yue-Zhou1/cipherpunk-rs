use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow};
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
use knowledge::{AuditMemoryEntry, FindingSeverityCounts, LongTermMemory, WorkingMemory};
use llm::{
    AdviserAction, AdviserBudgetSnapshot, AdviserContext, AdviserService, LlmProvider,
    ProviderFailoverRecord,
};
use report::generator::{ReportGenerator, ReportGeneratorOptions};
use research::{ResearchQuery, ResearchService};
use session_store::{SessionEvent, SessionStore};
use tokio::sync::Mutex as AsyncMutex;

pub mod events;
pub mod jobs;
mod runtime;
pub mod tool_actions;

pub use audit_agent_core::tooling::{ToolActionRequest, ToolActionResult, ToolFamily};
pub use events::{AuditEvent, AuditEventSink, JobLifecycleEvent};
pub use jobs::{AuditJob, AuditJobKind, AuditJobStatus};
pub use runtime::OrchestratorRuntime;
pub use tool_actions::plan_tool_action;

const MAX_ADVISER_CALLS_PER_AUDIT: u8 = 5;
const MAX_RETRIES_PER_ENGINE: u8 = 1;

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
    pub adviser: Option<AdviserService>,
    pub failover_events: Arc<Mutex<Vec<ProviderFailoverRecord>>>,
    working_memory: Arc<Mutex<WorkingMemory>>,
    pub long_term_memory: Option<Arc<tokio::sync::Mutex<LongTermMemory>>>,
    research_service: Option<Arc<ResearchService>>,
    run_lock: Arc<AsyncMutex<()>>,
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
            adviser: None,
            failover_events: Arc::new(Mutex::new(Vec::new())),
            working_memory: Arc::new(Mutex::new(WorkingMemory::new())),
            long_term_memory: None,
            research_service: ResearchService::new().ok().map(Arc::new),
            run_lock: Arc::new(AsyncMutex::new(())),
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

    pub fn with_adviser(mut self, adviser: AdviserService) -> Self {
        self.adviser = Some(adviser);
        self
    }

    pub fn with_failover_events(
        mut self,
        failover_events: Arc<Mutex<Vec<ProviderFailoverRecord>>>,
    ) -> Self {
        self.failover_events = failover_events;
        self
    }

    pub fn with_long_term_memory(
        mut self,
        memory: Arc<tokio::sync::Mutex<LongTermMemory>>,
    ) -> Self {
        self.long_term_memory = Some(memory);
        self
    }

    pub fn with_research_service(mut self, research_service: Arc<ResearchService>) -> Self {
        self.research_service = Some(research_service);
        self
    }

    pub fn llm_assist_context(&self, role_name: &str) -> Option<String> {
        self.working_memory
            .lock()
            .ok()
            .map(|working_memory| working_memory.context_for_role(role_name))
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

        if request.tool_family == ToolFamily::Research {
            let _ = std::fs::create_dir_all(&artifact_root);
            let research = self
                .research_service
                .as_ref()
                .ok_or_else(|| anyhow!("research service is unavailable"))?;
            let target = request.target.display_value().to_string();
            let result = research
                .query(&ResearchQuery::RustSecAdvisory {
                    crate_name: target.clone(),
                })
                .await?;
            let artifact_path =
                artifact_root.join(format!("research-{}.json", request.target.slug()));
            std::fs::write(&artifact_path, serde_json::to_vec_pretty(&result)?)?;

            let action_result = ToolActionResult {
                action_id: format!("action-{}", Utc::now().timestamp_micros()),
                session_id: request.session_id.clone(),
                tool_family: ToolFamily::Research,
                target: request.target,
                command: vec!["research".to_string(), target],
                artifact_refs: vec![artifact_path.to_string_lossy().to_string()],
                rationale: "Queried bounded advisory sources for target crate".to_string(),
                status: ToolActionStatus::Completed,
                stdout_preview: Some(format!(
                    "Research completed with {} finding(s)",
                    result.findings.len()
                )),
                stderr_preview: None,
            };

            if let Ok(mut working_memory) = self.working_memory.lock() {
                working_memory.record_tool_result(
                    "Research",
                    action_result.target.display_value(),
                    "completed",
                );
            }

            if let Some(store) = &self.session_store {
                let payload = serde_json::to_string(&action_result)?;
                let event = SessionEvent {
                    event_id: format!("tool-action:{}", action_result.action_id),
                    event_type: "tool.action".to_string(),
                    payload,
                    created_at: Utc::now(),
                };
                store.append_event(&action_result.session_id, &event)?;
            }

            return Ok(action_result);
        }

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

        if let Ok(mut working_memory) = self.working_memory.lock() {
            working_memory.record_tool_result(
                &format!("{:?}", result.tool_family),
                result.target.display_value(),
                match result.status {
                    ToolActionStatus::Completed => "completed",
                    ToolActionStatus::Failed => "failed",
                },
            );
        }

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
        let _run_guard = self.run_lock.lock().await;
        if let Ok(mut working_memory) = self.working_memory.lock() {
            *working_memory = WorkingMemory::new();
        }
        self.failover_events
            .lock()
            .expect("failover events lock")
            .clear();
        let dag = self.build_dag(config);
        let (findings, outcomes) = self.execute_dag(&dag, config).await?;
        let outputs = self.produce_outputs(&findings, &outcomes, config).await?;

        if let Some(long_term) = &self.long_term_memory {
            let entry = AuditMemoryEntry {
                audit_id: config.audit_id.clone(),
                timestamp: Utc::now().to_rfc3339(),
                source_description: format!("{:?}", config.source.origin),
                findings_by_severity: FindingSeverityCounts {
                    critical: outputs.manifest.finding_counts.critical,
                    high: outputs.manifest.finding_counts.high,
                    medium: outputs.manifest.finding_counts.medium,
                    low: outputs.manifest.finding_counts.low,
                    observation: outputs.manifest.finding_counts.observation,
                },
                engines_used: outcomes
                    .iter()
                    .map(|outcome| outcome.engine.clone())
                    .collect(),
                key_findings: outputs
                    .findings
                    .iter()
                    .take(5)
                    .map(|finding| finding.title.clone())
                    .collect(),
                tags: config
                    .scope
                    .detected_frameworks
                    .iter()
                    .map(|framework| format!("{framework:?}").to_ascii_lowercase())
                    .collect(),
            };
            let mut memory = long_term.lock().await;
            memory.record_audit_outcome(entry);
            if let Err(err) = memory.persist() {
                tracing::warn!(error = %err, "failed to persist long-term memory");
            }
        }

        Ok(outputs)
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

        let mut working_memory = WorkingMemory::new();
        let mut findings = Vec::<Finding>::new();
        let mut outcomes = Vec::<EngineOutcome>::new();
        let mut adviser_calls_remaining = MAX_ADVISER_CALLS_PER_AUDIT;
        for engine in &self.engines {
            let engine_name = engine.name().to_string();
            let started_at = std::time::Instant::now();

            if !engine.supports(&ctx).await {
                working_memory.record_engine_outcome(&engine_name, "skipped", 0);
                outcomes.push(EngineOutcome {
                    engine: engine_name,
                    status: EngineStatus::Skipped {
                        reason: "engine reported unsupported for this context".to_string(),
                    },
                    findings_count: 0,
                    duration_ms: started_at.elapsed().as_millis() as u64,
                    adviser_suggestion: None,
                });
                continue;
            }

            let mut attempt: u8 = 1;
            let mut attempt_config = config.clone();
            loop {
                let attempt_ctx = AuditContext {
                    config: Arc::new(attempt_config.clone()),
                    workspace: Arc::clone(&ctx.workspace),
                    sandbox: Arc::clone(&ctx.sandbox),
                    evidence_store: Arc::clone(&ctx.evidence_store),
                    llm: ctx.llm.clone(),
                };

                match engine.analyze(&attempt_ctx).await {
                    Ok(engine_findings) => {
                        let count = engine_findings.len();
                        let duration_ms = started_at.elapsed().as_millis() as u64;
                        for finding in &engine_findings {
                            working_memory.record_finding(finding);
                        }
                        working_memory.record_engine_outcome(&engine_name, "completed", count);
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
                            adviser_suggestion: None,
                        });
                        break;
                    }
                    Err(err) => {
                        let duration_ms = started_at.elapsed().as_millis() as u64;
                        let mut suggestion = None::<llm::AdviserSuggestion>;
                        let mut suggestion_applied = false;

                        if adviser_calls_remaining > 0 {
                            if let Some(adviser) = &self.adviser {
                                adviser_calls_remaining = adviser_calls_remaining.saturating_sub(1);
                                let adviser_context = AdviserContext {
                                    engine_name: adviser_engine_label(engine_name.as_str())
                                        .to_string(),
                                    error_message: err.to_string(),
                                    attempt_number: attempt,
                                    elapsed_ms: duration_ms,
                                    findings_so_far: findings.len(),
                                    budget: AdviserBudgetSnapshot::from_engine(
                                        engine_name.as_str(),
                                        &attempt_config.budget,
                                    ),
                                };
                                match adviser.suggest_on_failure(&adviser_context).await {
                                    Ok(value) => {
                                        suggestion = Some(value);
                                    }
                                    Err(adviser_err) => {
                                        tracing::warn!(
                                            engine = %engine_name,
                                            error = %adviser_err,
                                            "adviser call failed — continuing without suggestion"
                                        );
                                    }
                                }
                            }
                        }

                        if attempt <= MAX_RETRIES_PER_ENGINE && is_retryable_engine_failure(&err) {
                            if let Some(llm::AdviserSuggestion {
                                action:
                                    AdviserAction::RetryWithRelaxedBudget {
                                        timeout_secs,
                                        memory_mb,
                                    },
                                ..
                            }) = suggestion.as_ref()
                            {
                                if engine_supports_budget_adjustment(engine_name.as_str()) {
                                    match apply_retry_budget_adjustment(
                                        &mut attempt_config,
                                        engine_name.as_str(),
                                        *timeout_secs,
                                        *memory_mb,
                                    ) {
                                        Ok(true) => {
                                            suggestion_applied = true;
                                            attempt = attempt.saturating_add(1);
                                        }
                                        Ok(false) => {
                                            tracing::info!(
                                                engine = %engine_name,
                                                timeout_secs,
                                                memory_mb,
                                                "adviser retry suggestion did not change effective engine budget"
                                            );
                                        }
                                        Err(adjustment_err) => {
                                            tracing::warn!(
                                                engine = %engine_name,
                                                error = %adjustment_err,
                                                "failed to apply adviser retry suggestion"
                                            );
                                        }
                                    }
                                }
                            }
                        }

                        if let Some(sink) = &self.event_sink {
                            if let Some(ref value) = suggestion {
                                sink.emit(AuditEvent::AdviserConsulted {
                                    engine: engine_name.clone(),
                                    suggestion: format!("{:?}", value.action),
                                    applied: suggestion_applied,
                                });
                            }
                        }

                        if suggestion_applied {
                            continue;
                        }

                        if let Some(sink) = &self.event_sink {
                            sink.emit(AuditEvent::EngineFailed {
                                engine: engine_name.clone(),
                                reason: err.to_string(),
                            });
                        }

                        working_memory.record_engine_outcome(&engine_name, "failed", 0);
                        working_memory
                            .record_adviser_note(&format!("{engine_name} failed: {err}"));
                        outcomes.push(EngineOutcome {
                            engine: engine_name,
                            status: EngineStatus::Failed {
                                reason: err.to_string(),
                            },
                            findings_count: 0,
                            duration_ms,
                            adviser_suggestion: suggestion
                                .as_ref()
                                .map(|value| format!("{:?}: {}", value.action, value.rationale)),
                        });
                        break;
                    }
                }
            }
        }
        if let Ok(mut state) = self.working_memory.lock() {
            *state = working_memory;
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
        let mut coverage = CoverageReport::from_outcomes(outcomes);
        let failover_warnings = failover_warning_messages(&self.failover_events);
        if let Some(sink) = &self.event_sink {
            let events = self.failover_events.lock().expect("failover events lock");
            for event in events.iter() {
                sink.emit(AuditEvent::ProviderFailover {
                    from: event.from.clone(),
                    to: event.to.clone(),
                    role: event.role.clone(),
                    reason: event.reason.clone(),
                });
            }
        }
        if !failover_warnings.is_empty() {
            coverage.failover_warnings = failover_warnings.clone();
            coverage.warnings.extend(failover_warnings);
        }
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

fn is_retryable_engine_failure(err: &anyhow::Error) -> bool {
    let message = err.to_string().to_ascii_lowercase();
    if message.contains("unsupported")
        || message.contains("not supported")
        || message.contains("invalid input")
        || message.contains("semantic")
        || message.contains("unimplemented")
    {
        return false;
    }

    message.contains("timeout")
        || message.contains("timed out")
        || message.contains("oom")
        || message.contains("out of memory")
        || message.contains("resource exhausted")
        || message.contains("sandbox")
        || message.contains("temporary")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BudgetAdjustmentTarget {
    Kani,
    Z3Like,
    Fuzz,
    Madsim,
    Semantic,
}

fn budget_adjustment_target(engine_name: &str) -> Option<BudgetAdjustmentTarget> {
    let name = engine_name.to_ascii_lowercase();
    if name.contains("kani") {
        return Some(BudgetAdjustmentTarget::Kani);
    }
    if name.contains("z3") || name.contains("smt") {
        return Some(BudgetAdjustmentTarget::Z3Like);
    }
    if name.contains("fuzz") {
        return Some(BudgetAdjustmentTarget::Fuzz);
    }
    if name.contains("madsim") {
        return Some(BudgetAdjustmentTarget::Madsim);
    }
    if name.contains("semantic") {
        return Some(BudgetAdjustmentTarget::Semantic);
    }
    None
}

fn engine_supports_budget_adjustment(engine_name: &str) -> bool {
    budget_adjustment_target(engine_name).is_some()
}

fn adviser_engine_label(engine_name: &str) -> &'static str {
    match budget_adjustment_target(engine_name) {
        Some(BudgetAdjustmentTarget::Kani) => "kani",
        Some(BudgetAdjustmentTarget::Z3Like) => "smt",
        Some(BudgetAdjustmentTarget::Fuzz) => "fuzz",
        Some(BudgetAdjustmentTarget::Madsim) => "madsim",
        Some(BudgetAdjustmentTarget::Semantic) => "semantic-index",
        None => "generic-engine",
    }
}

fn apply_retry_budget_adjustment(
    config: &mut AuditConfig,
    engine_name: &str,
    timeout_secs: u64,
    memory_mb: u64,
) -> Result<bool> {
    let Some(target) = budget_adjustment_target(engine_name) else {
        anyhow::bail!("engine '{engine_name}' does not support budget adjustment");
    };

    let timeout_applied = match target {
        BudgetAdjustmentTarget::Kani => {
            let before = config.budget.kani_timeout_secs;
            config.budget.kani_timeout_secs = before.max(timeout_secs);
            config.budget.kani_timeout_secs > before
        }
        BudgetAdjustmentTarget::Z3Like => {
            let before = config.budget.z3_timeout_secs;
            config.budget.z3_timeout_secs = before.max(timeout_secs);
            config.budget.z3_timeout_secs > before
        }
        BudgetAdjustmentTarget::Fuzz => {
            let before = config.budget.fuzz_duration_secs;
            config.budget.fuzz_duration_secs = before.max(timeout_secs);
            config.budget.fuzz_duration_secs > before
        }
        BudgetAdjustmentTarget::Madsim => {
            let before = config.budget.madsim_ticks;
            config.budget.madsim_ticks = before.max(timeout_secs);
            config.budget.madsim_ticks > before
        }
        BudgetAdjustmentTarget::Semantic => {
            let before = config.budget.semantic_index_timeout_secs;
            config.budget.semantic_index_timeout_secs = before.max(timeout_secs);
            config.budget.semantic_index_timeout_secs > before
        }
    };

    if memory_mb > 0 {
        tracing::debug!(
            engine = %engine_name,
            memory_mb,
            "memory budget suggestions are currently advisory-only: no per-engine memory knob in AuditConfig"
        );
    }

    Ok(timeout_applied)
}

fn failover_warning_messages(
    failover_events: &Arc<Mutex<Vec<ProviderFailoverRecord>>>,
) -> Vec<String> {
    let events = failover_events.lock().expect("failover events lock");
    let mut warnings = events
        .iter()
        .map(|event| {
            format!(
                "LLM provider failover occurred: {} switched from {} to {}. Findings produced during failover may differ from baseline.",
                event.role, event.from, event.to
            )
        })
        .collect::<Vec<_>>();
    warnings.sort();
    warnings.dedup();
    warnings
}
