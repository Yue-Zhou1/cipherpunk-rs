use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use audit_agent_core::audit_config::{AuditConfig, ParsedPreviousAudit};
use audit_agent_core::engine::{AuditContext, AuditEngine};
use audit_agent_core::finding::Finding;
use audit_agent_core::output::{AuditManifest, AuditOutputs, FindingCounts};
use audit_agent_core::{EvidenceStore, SandboxExecutor};
use chrono::Utc;
use findings::pipeline::{
    deduplicate_findings, mark_regression_checks as mark_regression_checks_by_key,
};
use intake::diff::AnalysisCache;
use intake::summarize_optional_inputs;
use intake::workspace::WorkspaceAnalyzer;
use llm::LlmProvider;
use report::generator::{ReportGenerator, ReportGeneratorOptions};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditDag {
    pub nodes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditEvent {
    AuditCompleted {
        audit_id: String,
        output_dir: PathBuf,
        finding_count: usize,
    },
}

pub trait AuditEventSink: Send + Sync {
    fn emit(&self, event: AuditEvent);
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
    pub sandbox: Arc<SandboxExecutor>,
    pub evidence_store: Arc<EvidenceStore>,
    pub findings_db: FindingsDb,
    pub cache: Arc<AnalysisCache>,
    pub output_dir: PathBuf,
    pub evidence_pack_zip: PathBuf,
    pub llm: Option<Arc<dyn LlmProvider>>,
    pub event_sink: Option<Arc<dyn AuditEventSink>>,
}

impl AuditOrchestrator {
    pub fn new(output_dir: PathBuf, evidence_pack_zip: PathBuf) -> Self {
        Self {
            engines: vec![],
            sandbox: Arc::new(SandboxExecutor),
            evidence_store: Arc::new(EvidenceStore),
            findings_db: FindingsDb,
            cache: Arc::new(AnalysisCache::default()),
            output_dir,
            evidence_pack_zip,
            llm: None,
            event_sink: None,
        }
    }

    pub fn with_engines(mut self, engines: Vec<Box<dyn AuditEngine>>) -> Self {
        self.engines = engines;
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

    pub async fn run(&self, config: &AuditConfig) -> Result<AuditOutputs> {
        let dag = self.build_dag(config);
        let findings = self.execute_dag(&dag, config).await?;
        self.produce_outputs(&findings, config).await
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

    pub async fn execute_dag(&self, _dag: &AuditDag, config: &AuditConfig) -> Result<Vec<Finding>> {
        let workspace = Arc::new(WorkspaceAnalyzer::analyze(&config.source.local_path)?);
        let ctx = AuditContext {
            config: Arc::new(config.clone()),
            workspace,
            sandbox: Arc::clone(&self.sandbox),
            evidence_store: Arc::clone(&self.evidence_store),
            llm: None,
        };

        let mut findings = Vec::<Finding>::new();
        for engine in &self.engines {
            if engine.supports(&ctx).await {
                findings.extend(engine.analyze(&ctx).await?);
            }
        }
        Ok(findings)
    }

    pub async fn produce_outputs(
        &self,
        findings: &[Finding],
        config: &AuditConfig,
    ) -> Result<AuditOutputs> {
        let mut deduplicated = self.findings_db.deduplicate(findings);
        self.findings_db.mark_regression_checks(
            &mut deduplicated,
            config.optional_inputs.previous_audit.as_ref(),
        );

        let finding_counts = FindingCounts::from(&deduplicated);
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
            engines_run: self
                .engines
                .iter()
                .map(|engine| engine.name().to_string())
                .collect(),
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
            engines_run: self
                .engines
                .iter()
                .map(|engine| engine.name().to_string())
                .collect(),
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
        })
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
