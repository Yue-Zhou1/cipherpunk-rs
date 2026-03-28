use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use audit_agent_core::audit_config::{AuditConfig, BudgetConfig, ResolvedSource, SourceOrigin};
use audit_agent_core::finding::{Severity, VerificationStatus};
use audit_agent_core::output::{AuditManifest, CoverageReport};
use audit_agent_core::session::{
    AuditPlan, AuditPlanDomain, AuditPlanEngines, AuditPlanOverview, AuditPlanTool, AuditRecord,
    AuditRecordKind, AuditSession, SessionUiState,
};
use audit_agent_core::tooling::{ToolActionResult, ToolActionStatus, ToolFamily};
use chrono::Utc;
use engine_crypto::intake_bridge::{CryptoEngineContext, EnvironmentManifest};
use engine_crypto::rules::RuleEvaluator;
use intake::config::{
    ConfigParser, RawEngineConfig, RawLlmConfig, RawScope, RawSource, ValidatedConfig,
};
use intake::confirmation::{ConfirmationSummary, UserDecisions};
use intake::project_snapshot_from_config;
use intake::source::SourceInput;
use intake::workspace::WorkspaceAnalyzer;
use knowledge::KnowledgeBase;
use knowledge::memory_block::MemoryBlock;
use knowledge::memory_block::embedder::resolved_config_and_provider_from_env;
use knowledge::models::AdjudicatedCase;
use llm::{LlmInteractionHook, LlmProvenance};
use orchestrator::{AuditJob, AuditJobKind, AuditOrchestrator};
use project_ir::{
    ChecklistPlan as IrChecklistPlan, ProjectIr, ProjectIrBuilder,
    SecurityOverview as IrSecurityOverview,
};
use research::{ResearchQuery, ResearchService};
use serde::{Deserialize, Serialize};
use session_store::{LlmInteractionEvent, SessionStore};

use crate::explorer_graph::{ExplorerDepth, ExplorerGraphBuilder, ExplorerGraphResponse};
use crate::{
    ActivitySummary, AuditPlanDomainView, AuditPlanOverviewView, AuditPlanResponse,
    ConfigParseResponse, EngineOutcomeView, EngineSelectionView, LlmCallSummary, OutputType,
    ResolvedSourceView, ReviewDecisionSummary, ToolActionSummary, ToolRecommendationView,
    confirm_workspace, detect_workspace, download_output, export_audit_yaml, get_audit_manifest,
    resolve_source,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceInputIpc {
    pub kind: SourceKind,
    pub value: String,
    pub commit_or_ref: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceKind {
    Git,
    Local,
    Archive,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmWorkspaceRequest {
    pub confirmed: bool,
    pub ambiguous_crates: HashMap<String, bool>,
    #[serde(default)]
    pub no_llm_prose: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmWorkspaceResponse {
    pub audit_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadOutputResponse {
    pub dest: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionJobView {
    pub job_id: String,
    pub kind: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateAuditSessionResponse {
    pub session_id: String,
    pub snapshot_id: String,
    pub initial_jobs: Vec<SessionJobView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditSessionSummary {
    pub session_id: String,
    pub snapshot_id: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenAuditSessionResponse {
    pub session_id: String,
    pub snapshot_id: String,
    pub selected_domains: Vec<String>,
    pub initial_jobs: Vec<SessionJobView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectTreeNodeKind {
    Directory,
    File,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectTreeNode {
    pub name: String,
    pub path: String,
    pub kind: ProjectTreeNodeKind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<ProjectTreeNode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetProjectTreeResponse {
    pub session_id: String,
    pub root_name: String,
    pub nodes: Vec<ProjectTreeNode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadSourceFileResponse {
    pub session_id: String,
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionConsoleLevel {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionConsoleEntry {
    pub timestamp: String,
    pub source: String,
    pub level: SessionConsoleLevel,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TailSessionConsoleResponse {
    pub session_id: String,
    pub entries: Vec<SessionConsoleEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectGraphNodeResponse {
    pub id: String,
    pub label: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finding_count: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_severity: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectGraphEdgeResponse {
    pub from: String,
    pub to: String,
    pub relation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_preview: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectGraphResponse {
    pub session_id: String,
    pub lens: String,
    pub redacted_values: bool,
    pub nodes: Vec<ProjectGraphNodeResponse>,
    pub edges: Vec<ProjectGraphEdgeResponse>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadSecurityOverviewResponse {
    pub session_id: String,
    pub assets: Vec<String>,
    pub trust_boundaries: Vec<String>,
    pub hotspots: Vec<String>,
    pub review_notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChecklistDomainPlanResponse {
    pub id: String,
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadChecklistPlanResponse {
    pub session_id: String,
    pub domains: Vec<ChecklistDomainPlanResponse>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolbenchSelectionRequest {
    pub kind: String,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolbenchSelectionResponse {
    pub kind: String,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolbenchRecommendationResponse {
    pub tool_id: String,
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolbenchSimilarCaseResponse {
    pub id: String,
    pub title: String,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchAdvisoryView {
    pub source: String,
    pub id: String,
    pub title: String,
    pub severity: Option<String>,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadToolbenchContextResponse {
    pub session_id: String,
    pub selection: ToolbenchSelectionResponse,
    pub recommended_tools: Vec<ToolbenchRecommendationResponse>,
    pub domains: Vec<ChecklistDomainPlanResponse>,
    pub overview_notes: Vec<String>,
    pub similar_cases: Vec<ToolbenchSimilarCaseResponse>,
    #[serde(default)]
    pub advisories: Vec<ResearchAdvisoryView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewDecisionAction {
    Confirm,
    Reject,
    Suppress,
    Annotate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyReviewDecisionRequest {
    pub record_id: String,
    pub action: ReviewDecisionAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewQueueItemResponse {
    pub record_id: String,
    pub kind: String,
    pub title: String,
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity: Option<String>,
    pub verification_status: String,
    pub labels: Vec<String>,
    pub evidence_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ir_node_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadReviewQueueResponse {
    pub session_id: String,
    pub items: Vec<ReviewQueueItemResponse>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyReviewDecisionResponse {
    pub session_id: String,
    pub item: ReviewQueueItemResponse,
}

pub struct UiSessionState {
    work_dir: PathBuf,
    resolved_source: Option<ResolvedSourceView>,
    validated_config: Option<ValidatedConfig>,
    confirmation_summary: Option<ConfirmationSummary>,
    audit_config: Option<AuditConfig>,
    active_session_id: Option<String>,
    sessions: HashMap<String, AuditSession>,
    session_jobs: HashMap<String, Vec<SessionJobView>>,
    project_ir_cache: HashMap<String, ProjectIr>,
    review_records_cache: HashMap<String, Vec<AuditRecord>>,
    graph_overlay_cache: HashMap<String, GraphOverlayCacheEntry>,
    session_store: Option<Arc<SessionStore>>,
    research_service: Option<Arc<ResearchService>>,
}

#[derive(Debug, Clone)]
struct GraphOverlayCacheEntry {
    loaded_at: Instant,
    records: Vec<AuditRecord>,
}

const GRAPH_OVERLAY_CACHE_TTL: Duration = Duration::from_secs(2);

impl UiSessionState {
    pub fn new(work_dir: PathBuf) -> Self {
        let session_store = SessionStore::open(work_dir.join("sessions.sqlite"))
            .ok()
            .map(Arc::new);
        Self {
            work_dir,
            resolved_source: None,
            validated_config: None,
            confirmation_summary: None,
            audit_config: None,
            active_session_id: None,
            sessions: HashMap::new(),
            session_jobs: HashMap::new(),
            project_ir_cache: HashMap::new(),
            review_records_cache: HashMap::new(),
            graph_overlay_cache: HashMap::new(),
            session_store,
            research_service: ResearchService::new().ok().map(Arc::new),
        }
    }

    pub fn resolved_source(&self) -> Option<&ResolvedSourceView> {
        self.resolved_source.as_ref()
    }

    pub fn set_resolved_source(&mut self, resolved_source: ResolvedSourceView) {
        self.resolved_source = Some(resolved_source);
        self.confirmation_summary = None;
        self.audit_config = None;
    }

    pub fn set_validated_config(&mut self, validated_config: ValidatedConfig) {
        self.validated_config = Some(validated_config);
    }

    pub fn set_confirmation_summary(&mut self, summary: ConfirmationSummary) {
        self.confirmation_summary = Some(summary);
    }

    pub fn audit_config(&self) -> Option<&AuditConfig> {
        self.audit_config.as_ref()
    }

    pub async fn resolve_source(&mut self, input: SourceInputIpc) -> Result<ResolvedSourceView> {
        let source_input = input.into_source_input()?;
        let resolved = resolve_source(source_input, &self.work_dir).await?;
        self.resolved_source = Some(resolved.clone());
        self.confirmation_summary = None;
        self.audit_config = None;
        Ok(resolved)
    }

    pub fn parse_config(&mut self, path: &Path) -> ConfigParseResponse {
        match ConfigParser::parse(path) {
            Ok(validated) => {
                let response = ConfigParseResponse::Validated {
                    target_crates: validated.scope.target_crates.clone(),
                    exclude_crates: validated.scope.exclude_crates.clone(),
                    output_dir: validated.output_dir.clone(),
                };
                self.validated_config = Some(validated);
                response
            }
            Err(errors) => ConfigParseResponse::ConfigErrors {
                errors: errors.into_iter().map(|error| format!("{error}")).collect(),
            },
        }
    }

    pub fn detect_workspace(&mut self) -> Result<ConfirmationSummary> {
        let source = self
            .resolved_source
            .as_ref()
            .context("resolve_source must be called before detect_workspace")?;
        let summary = detect_workspace(&source.source)?;
        self.confirmation_summary = Some(summary.clone());
        Ok(summary)
    }

    pub fn confirm_workspace(
        &mut self,
        request: ConfirmWorkspaceRequest,
    ) -> Result<ConfirmWorkspaceResponse> {
        let source = self
            .resolved_source
            .as_ref()
            .context("resolve_source must be called before confirm_workspace")?;

        let summary = match self.confirmation_summary.clone() {
            Some(summary) => summary,
            None => {
                let summary = detect_workspace(&source.source)?;
                self.confirmation_summary = Some(summary.clone());
                summary
            }
        };

        let validated = self
            .validated_config
            .clone()
            .unwrap_or_else(|| default_validated_config(&source.source));

        let decisions = UserDecisions {
            ambiguous_crates: request.ambiguous_crates,
            override_features: None,
            confirmed: request.confirmed,
            export_audit_yaml: false,
        };

        let config = confirm_workspace(
            decisions,
            source.source.clone(),
            validated,
            summary,
            request.no_llm_prose,
        )?;
        let response = ConfirmWorkspaceResponse {
            audit_id: config.audit_id.clone(),
        };

        self.audit_config = Some(config);
        Ok(response)
    }

    pub fn export_audit_yaml(&self, path: &Path) -> Result<()> {
        let config = self
            .audit_config
            .as_ref()
            .context("confirm_workspace must be called before export_audit_yaml")?;
        export_audit_yaml(config, path)
    }

    pub fn get_audit_manifest(&self) -> Result<AuditManifest> {
        let config = self
            .audit_config
            .as_ref()
            .context("confirm_workspace must be called before get_audit_manifest")?;
        get_audit_manifest(&config.output_dir)
    }

    pub fn download_output(
        &self,
        audit_id: &str,
        output_type: OutputType,
        dest: &Path,
    ) -> Result<DownloadOutputResponse> {
        let config = self
            .audit_config
            .as_ref()
            .context("confirm_workspace must be called before download_output")?;

        if config.audit_id != audit_id {
            bail!(
                "requested audit_id `{audit_id}` does not match active audit `{}`",
                config.audit_id
            );
        }

        download_output(&config.output_dir, output_type, dest)?;
        Ok(DownloadOutputResponse {
            dest: dest.to_path_buf(),
        })
    }

    pub async fn create_audit_session(&mut self) -> Result<CreateAuditSessionResponse> {
        let config = self
            .audit_config
            .as_ref()
            .context("confirm_workspace must be called before create_audit_session")?
            .clone();

        let session_id = make_session_id();
        let snapshot_id = make_snapshot_id();
        let now = Utc::now();
        let session = AuditSession {
            session_id: session_id.clone(),
            snapshot: project_snapshot_from_config(&config, snapshot_id.clone()),
            selected_domains: config
                .scope
                .detected_frameworks
                .iter()
                .map(|framework| format!("{framework:?}").to_ascii_lowercase())
                .collect(),
            ui_state: SessionUiState::default(),
            created_at: now,
            updated_at: now,
        };

        if let Some(store) = &self.session_store {
            store.create_session(&session)?;
        }
        self.active_session_id = Some(session_id.clone());
        self.sessions.insert(session_id.clone(), session.clone());

        let orchestrator = session_orchestrator(&self.work_dir, self.session_store.clone());
        let jobs = orchestrator.bootstrap_jobs(&session).await?;
        let initial_jobs = jobs.iter().map(job_to_view).collect::<Vec<_>>();
        self.session_jobs
            .insert(session_id.clone(), initial_jobs.clone());

        Ok(CreateAuditSessionResponse {
            session_id,
            snapshot_id,
            initial_jobs,
        })
    }

    pub async fn create_audit_session_for_tests(&mut self) -> Result<CreateAuditSessionResponse> {
        if self.audit_config.is_none() {
            self.audit_config = Some(test_audit_config(&self.work_dir));
        }
        self.create_audit_session().await
    }

    pub fn list_audit_sessions(&self) -> Result<Vec<AuditSessionSummary>> {
        let mut by_id = self
            .sessions
            .iter()
            .map(|(session_id, session)| (session_id.clone(), session.clone()))
            .collect::<HashMap<_, _>>();
        if let Some(store) = &self.session_store {
            for session in store.list_sessions()? {
                by_id.entry(session.session_id.clone()).or_insert(session);
            }
        }

        let mut sessions = by_id
            .values()
            .map(|session| AuditSessionSummary {
                session_id: session.session_id.clone(),
                snapshot_id: session.snapshot.snapshot_id.clone(),
                created_at: session.created_at.to_rfc3339(),
                updated_at: session.updated_at.to_rfc3339(),
            })
            .collect::<Vec<_>>();
        sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(sessions)
    }

    pub async fn open_audit_session(
        &mut self,
        session_id: &str,
    ) -> Result<Option<OpenAuditSessionResponse>> {
        let session = if let Some(existing) = self.sessions.get(session_id) {
            existing.clone()
        } else if let Some(store) = &self.session_store {
            let Some(loaded) = store.load_session(session_id)? else {
                return Ok(None);
            };
            self.sessions.insert(session_id.to_string(), loaded.clone());
            loaded
        } else {
            return Ok(None);
        };

        self.active_session_id = Some(session.session_id.clone());

        let initial_jobs = if let Some(cached_jobs) = self.session_jobs.get(&session.session_id) {
            cached_jobs.clone()
        } else if let Some(store) = &self.session_store {
            let loaded = session_jobs_from_events(&store.list_events(&session.session_id)?);
            if loaded.is_empty() {
                vec![]
            } else {
                self.session_jobs
                    .insert(session.session_id.clone(), loaded.clone());
                loaded
            }
        } else {
            vec![]
        };

        Ok(Some(OpenAuditSessionResponse {
            session_id: session.session_id,
            snapshot_id: session.snapshot.snapshot_id,
            selected_domains: session.selected_domains,
            initial_jobs,
        }))
    }

    pub async fn get_project_tree(&mut self, session_id: &str) -> Result<GetProjectTreeResponse> {
        let session = self.ensure_session_loaded(session_id)?;
        let root_dir = canonical_session_root(&session)?;
        let root_name = root_dir
            .file_name()
            .map(|value| value.to_string_lossy().to_string())
            .unwrap_or_else(|| root_dir.display().to_string());
        let scoped_roots = scoped_tree_roots(&session, &root_dir);
        let scope = (!scoped_roots.is_empty()).then_some(scoped_roots.as_slice());

        let mut remaining = MAX_PROJECT_TREE_NODES;
        let nodes = collect_tree_nodes(&root_dir, &root_dir, 0, &mut remaining, scope)?;

        Ok(GetProjectTreeResponse {
            session_id: session_id.to_string(),
            root_name,
            nodes,
        })
    }

    pub async fn read_source_file(
        &mut self,
        session_id: &str,
        path: &str,
    ) -> Result<ReadSourceFileResponse> {
        let session = self.ensure_session_loaded(session_id)?;
        let root_dir = canonical_session_root(&session)?;
        let relative_path = normalized_relative_path(path)?;
        let candidate = root_dir.join(&relative_path);
        let canonical_file = fs::canonicalize(&candidate)
            .with_context(|| format!("read source file {}", candidate.display()))?;

        if !canonical_file.starts_with(&root_dir) {
            bail!("requested path is outside of the session source root");
        }

        let metadata = fs::metadata(&canonical_file)
            .with_context(|| format!("read file metadata {}", canonical_file.display()))?;
        if metadata.len() > MAX_SOURCE_FILE_BYTES {
            bail!("requested source file exceeds {MAX_SOURCE_FILE_BYTES} bytes");
        }

        let bytes = fs::read(&canonical_file)
            .with_context(|| format!("read {}", canonical_file.display()))?;
        let content = String::from_utf8(bytes).with_context(|| {
            format!(
                "source file is not valid utf-8 {}",
                canonical_file.display()
            )
        })?;

        Ok(ReadSourceFileResponse {
            session_id: session_id.to_string(),
            path: relative_path_to_string(&relative_path),
            content,
        })
    }

    pub fn tail_session_console(
        &mut self,
        session_id: &str,
        limit: usize,
    ) -> Result<TailSessionConsoleResponse> {
        let session = self.ensure_session_loaded(session_id)?;
        let requested = limit.max(1);
        let mut entries = Vec::<SessionConsoleEntry>::new();

        if let Some(store) = &self.session_store {
            let events = store.list_events(&session.session_id)?;
            let start = events.len().saturating_sub(requested);
            for event in &events[start..] {
                entries.push(console_entry_from_event(event));
            }
        }

        if entries.is_empty() {
            let jobs = self
                .session_jobs
                .get(&session.session_id)
                .cloned()
                .unwrap_or_default();
            for (index, job) in jobs.iter().take(requested).enumerate() {
                entries.push(SessionConsoleEntry {
                    timestamp: format!("bootstrap+{}", index + 1),
                    source: "job.bootstrap".to_string(),
                    level: SessionConsoleLevel::Info,
                    message: format!("{} [{}]", job.kind, job.status),
                });
            }
        }

        Ok(TailSessionConsoleResponse {
            session_id: session_id.to_string(),
            entries,
        })
    }

    pub fn load_activity_summary(&self, session_id: &str) -> Result<ActivitySummary> {
        self.ensure_session_exists(session_id)?;
        let store = self
            .session_store
            .as_ref()
            .ok_or_else(|| anyhow!("no session store"))?;
        let events = store.list_events(session_id)?;

        let mut llm_by_role = HashMap::<String, LlmRoleAggregate>::new();
        let mut tool_by_family = HashMap::<String, ToolFamilyAggregate>::new();
        let mut review_by_action = HashMap::<String, usize>::new();
        let mut engine_outcomes = Vec::<EngineOutcomeView>::new();
        let mut total_duration_ms = 0_u64;

        for event in &events {
            match event.event_type.as_str() {
                "llm.interaction" => {
                    if let Ok(data) =
                        serde_json::from_str::<LlmInteractionSummaryEvent>(&event.payload)
                    {
                        let aggregate = llm_by_role.entry(data.role).or_default();
                        aggregate.count += 1;
                        aggregate.total_duration_ms += data.duration_ms;
                        aggregate.total_prompt_chars += data.prompt_chars;
                        aggregate.total_response_chars += data.response_chars;
                        aggregate.providers_used.insert(data.provider);
                        if data.succeeded {
                            aggregate.succeeded += 1;
                        } else {
                            aggregate.failed += 1;
                        }
                        total_duration_ms += data.duration_ms;
                    }
                }
                "tool.action.completed" => {
                    if let Ok(data) =
                        serde_json::from_str::<ToolActionCompletedEvent>(&event.payload)
                    {
                        let aggregate = tool_by_family.entry(data.tool_family).or_default();
                        aggregate.count += 1;
                        aggregate.total_duration_ms += data.duration_ms;
                        if data.status.eq_ignore_ascii_case("completed") {
                            aggregate.succeeded += 1;
                        } else {
                            aggregate.failed += 1;
                        }
                        total_duration_ms += data.duration_ms;
                    }
                }
                "tool.action" => {
                    if let Ok(data) = serde_json::from_str::<ToolActionResult>(&event.payload) {
                        let tool_family = tool_family_name(&data.tool_family).to_string();
                        let aggregate = tool_by_family.entry(tool_family).or_default();
                        aggregate.count += 1;
                        if matches!(data.status, ToolActionStatus::Completed) {
                            aggregate.succeeded += 1;
                        } else {
                            aggregate.failed += 1;
                        }
                    }
                }
                "review.decision" | "review.action" => {
                    if let Ok(data) = serde_json::from_str::<ReviewDecisionEvent>(&event.payload) {
                        *review_by_action.entry(data.action).or_default() += 1;
                    }
                }
                "engine.completed" => {
                    if let Ok(data) = serde_json::from_str::<EngineCompletedEvent>(&event.payload) {
                        total_duration_ms += data.duration_ms;
                        engine_outcomes.push(EngineOutcomeView {
                            engine: data.engine,
                            status: "completed".to_string(),
                            findings_count: data.findings_count,
                            duration_ms: data.duration_ms,
                        });
                    }
                }
                "engine.failed" => {
                    if let Ok(data) = serde_json::from_str::<EngineFailedEvent>(&event.payload) {
                        engine_outcomes.push(EngineOutcomeView {
                            engine: data.engine,
                            status: "failed".to_string(),
                            findings_count: 0,
                            duration_ms: 0,
                        });
                    }
                }
                _ => {}
            }
        }

        let mut llm_calls = llm_by_role
            .into_iter()
            .map(|(role, aggregate)| {
                let mut providers_used = aggregate.providers_used.into_iter().collect::<Vec<_>>();
                providers_used.sort();
                let avg_duration_ms = if aggregate.count == 0 {
                    0
                } else {
                    aggregate.total_duration_ms / aggregate.count as u64
                };
                LlmCallSummary {
                    role,
                    count: aggregate.count,
                    avg_duration_ms,
                    total_prompt_chars: aggregate.total_prompt_chars,
                    total_response_chars: aggregate.total_response_chars,
                    providers_used,
                    succeeded: aggregate.succeeded,
                    failed: aggregate.failed,
                }
            })
            .collect::<Vec<_>>();
        llm_calls.sort_by(|a, b| a.role.cmp(&b.role));

        let mut tool_actions = tool_by_family
            .into_iter()
            .map(|(tool_family, aggregate)| {
                let avg_duration_ms = if aggregate.count == 0 {
                    0
                } else {
                    aggregate.total_duration_ms / aggregate.count as u64
                };
                ToolActionSummary {
                    tool_family,
                    count: aggregate.count,
                    succeeded: aggregate.succeeded,
                    failed: aggregate.failed,
                    avg_duration_ms,
                }
            })
            .collect::<Vec<_>>();
        tool_actions.sort_by(|a, b| a.tool_family.cmp(&b.tool_family));

        let mut review_decisions = review_by_action
            .into_iter()
            .map(|(action, count)| ReviewDecisionSummary { action, count })
            .collect::<Vec<_>>();
        review_decisions.sort_by(|a, b| a.action.cmp(&b.action));

        engine_outcomes.sort_by(|a, b| a.engine.cmp(&b.engine));

        Ok(ActivitySummary {
            session_id: session_id.to_string(),
            llm_calls,
            tool_actions,
            review_decisions,
            engine_outcomes,
            total_events: events.len(),
            total_duration_ms,
        })
    }

    pub async fn load_file_graph(&mut self, session_id: &str) -> Result<ProjectGraphResponse> {
        let source_root = self
            .ensure_session_loaded(session_id)?
            .snapshot
            .source
            .local_path
            .clone();
        let ir = self
            .load_or_build_project_ir(session_id, false)
            .await
            .map_err(map_project_ir_build_error)?;
        let mut response = file_graph_response(session_id, &ir);
        let records = self.load_graph_overlay_records(session_id)?;
        annotate_graph_with_findings(&mut response.nodes, &records, Some(source_root.as_path()));
        Ok(response)
    }

    pub async fn load_feature_graph(&mut self, session_id: &str) -> Result<ProjectGraphResponse> {
        let source_root = self
            .ensure_session_loaded(session_id)?
            .snapshot
            .source
            .local_path
            .clone();
        let ir = self
            .load_or_build_project_ir(session_id, false)
            .await
            .map_err(map_project_ir_build_error)?;
        let mut response = feature_graph_response(session_id, &ir);
        let records = self.load_graph_overlay_records(session_id)?;
        annotate_graph_with_findings(&mut response.nodes, &records, Some(source_root.as_path()));
        Ok(response)
    }

    pub async fn load_dataflow_graph(
        &mut self,
        session_id: &str,
        include_values: bool,
    ) -> Result<ProjectGraphResponse> {
        let source_root = self
            .ensure_session_loaded(session_id)?
            .snapshot
            .source
            .local_path
            .clone();
        let ir = self
            .load_or_build_project_ir(session_id, include_values)
            .await
            .map_err(map_project_ir_build_error)?;
        let mut response = dataflow_graph_response(session_id, &ir, !include_values);
        let records = self.load_graph_overlay_records(session_id)?;
        annotate_graph_with_findings(&mut response.nodes, &records, Some(source_root.as_path()));
        Ok(response)
    }

    pub async fn load_symbol_graph(&mut self, session_id: &str) -> Result<ProjectGraphResponse> {
        let source_root = self
            .ensure_session_loaded(session_id)?
            .snapshot
            .source
            .local_path
            .clone();
        let ir = self
            .load_or_build_project_ir(session_id, false)
            .await
            .map_err(map_project_ir_build_error)?;
        let mut response = symbol_graph_response(session_id, &ir);
        let records = self.load_graph_overlay_records(session_id)?;
        annotate_graph_with_findings(&mut response.nodes, &records, Some(source_root.as_path()));
        Ok(response)
    }

    pub async fn load_explorer_graph(
        &mut self,
        session_id: &str,
        depth: ExplorerDepth,
        cluster: Option<&str>,
    ) -> Result<ExplorerGraphResponse> {
        let source_root = self
            .ensure_session_loaded(session_id)?
            .snapshot
            .source
            .local_path
            .clone();
        let ir = if depth == ExplorerDepth::Overview && cluster.is_none() {
            self.load_or_refresh_project_ir_for_explorer(session_id)
                .await
                .map_err(map_project_ir_build_error)?
        } else {
            self.load_or_build_project_ir(session_id, false)
                .await
                .map_err(map_project_ir_build_error)?
        };
        let builder = ExplorerGraphBuilder::new(&ir, &source_root);
        builder
            .build(session_id, depth, cluster)
            .map_err(|err| anyhow!(err))
    }

    pub async fn load_security_overview(
        &mut self,
        session_id: &str,
    ) -> Result<LoadSecurityOverviewResponse> {
        let ir = self.load_or_build_project_ir(session_id, false).await?;
        let mut response = security_overview_response(session_id, ir.security_overview());
        let output_dir = self
            .audit_config
            .as_ref()
            .map(|config| config.output_dir.clone())
            .unwrap_or_else(|| self.work_dir.join("audit-output"));
        let coverage = get_audit_manifest(&output_dir)
            .ok()
            .and_then(|manifest| manifest.coverage);
        response.review_notes = prepend_coverage_warnings(response.review_notes, coverage.as_ref());
        Ok(response)
    }

    pub async fn load_checklist_plan(
        &mut self,
        session_id: &str,
    ) -> Result<LoadChecklistPlanResponse> {
        let ir = self.load_or_build_project_ir(session_id, false).await?;
        Ok(checklist_plan_response(session_id, ir.checklist_plan()))
    }

    pub fn load_audit_plan(&mut self, session_id: &str) -> Result<AuditPlanResponse> {
        let _session = self.ensure_session_loaded(session_id)?;
        let store = self
            .session_store
            .as_ref()
            .ok_or_else(|| anyhow!("no session store"))?;
        let events = store.list_events_by_type(session_id, "audit.plan.generated")?;
        let plan_event = events
            .last()
            .ok_or_else(|| anyhow!("audit plan not found for session `{session_id}`"))?;
        let plan: AuditPlan = serde_json::from_str(&plan_event.payload)
            .context("deserialize audit plan event payload")?;

        Ok(AuditPlanResponse {
            session_id: session_id.to_string(),
            plan_id: plan.plan_id,
            overview: AuditPlanOverviewView {
                assets: plan.overview.assets,
                trust_boundaries: plan.overview.trust_boundaries,
                hotspots: plan.overview.hotspots,
            },
            domains: plan
                .domains
                .into_iter()
                .map(|domain| AuditPlanDomainView {
                    id: domain.id,
                    rationale: domain.rationale,
                })
                .collect(),
            recommended_tools: plan
                .recommended_tools
                .into_iter()
                .map(|tool| ToolRecommendationView {
                    tool: tool.tool,
                    rationale: tool.rationale,
                })
                .collect(),
            engines: EngineSelectionView {
                crypto_zk: plan.engines.crypto_zk,
                distributed: plan.engines.distributed,
            },
            rationale: plan.rationale,
            created_at: plan.created_at.to_rfc3339(),
        })
    }

    pub async fn load_toolbench_context(
        &mut self,
        session_id: &str,
        selection: ToolbenchSelectionRequest,
    ) -> Result<LoadToolbenchContextResponse> {
        let session = self.ensure_session_loaded(session_id)?;
        let ir = self.load_or_build_project_ir(session_id, false).await?;
        let overview = ir.security_overview();
        let checklist = ir.checklist_plan();
        let context_terms = toolbench_context_terms(&selection, &checklist, &overview, &ir);

        let (mut recommended_tools, similar_cases) = match self.load_knowledge_base() {
            Ok(knowledge_base) => {
                let mut tools = Vec::<ToolbenchRecommendationResponse>::new();
                for tool in knowledge_base
                    .recommend_tools(&context_terms, &overview.review_notes)
                    .into_iter()
                {
                    push_recommendation(&mut tools, canonical_tool_id(&tool.tool), tool.rationale);
                }
                let cases = knowledge_base
                    .similar_cases(&context_terms, 3)
                    .into_iter()
                    .map(|case| ToolbenchSimilarCaseResponse {
                        id: case.id,
                        title: case.title,
                        summary: case.summary,
                    })
                    .collect::<Vec<_>>();
                (tools, cases)
            }
            Err(_) => (vec![], vec![]),
        };

        if recommended_tools.is_empty() {
            recommended_tools = fallback_tool_recommendations(&checklist);
        }

        let advisories = self.load_research_advisories(&session).await;

        if let Some(config) = self.audit_config.as_ref() {
            let should_persist_plan = if let Some(store) = &self.session_store {
                store
                    .list_events_by_type(&session.session_id, "audit.plan.generated")?
                    .is_empty()
            } else {
                !config.output_dir.join("audit-plan.json").exists()
            };
            if should_persist_plan {
                let plan = generate_audit_plan(
                    &session,
                    &overview,
                    &checklist,
                    &recommended_tools,
                    config,
                );
                self.persist_audit_plan(&session.session_id, &plan, &config.output_dir)?;
            }
        }

        Ok(LoadToolbenchContextResponse {
            session_id: session_id.to_string(),
            selection: ToolbenchSelectionResponse {
                kind: selection.kind,
                id: selection.id,
            },
            recommended_tools,
            domains: checklist_domain_responses(&checklist),
            overview_notes: overview.review_notes,
            similar_cases,
            advisories,
        })
    }

    fn persist_audit_plan(
        &self,
        session_id: &str,
        plan: &AuditPlan,
        output_dir: &Path,
    ) -> Result<()> {
        if let Some(store) = &self.session_store {
            let plan_event = session_store::SessionEvent {
                event_id: format!("audit-plan:{}", plan.plan_id),
                event_type: "audit.plan.generated".to_string(),
                payload: serde_json::to_string(plan)?,
                created_at: Utc::now(),
            };
            store.append_event(session_id, &plan_event)?;
        }

        fs::create_dir_all(output_dir)
            .with_context(|| format!("create output directory {}", output_dir.display()))?;
        fs::write(
            output_dir.join("audit-plan.json"),
            serde_json::to_string_pretty(plan)?,
        )
        .with_context(|| format!("write {}", output_dir.join("audit-plan.json").display()))?;
        Ok(())
    }

    pub async fn load_review_queue(&mut self, session_id: &str) -> Result<LoadReviewQueueResponse> {
        let _session = self.ensure_session_loaded(session_id)?;
        let mut records = self.load_candidate_records(session_id)?;

        if records.is_empty() {
            let seeded_from_rules = self.seed_rule_based_candidates(session_id).await?;
            if seeded_from_rules.is_empty() {
                let seeded = self.seed_default_candidate(session_id).await?;
                records.push(seeded);
            } else {
                records.extend(seeded_from_rules);
            }
        }

        Ok(LoadReviewQueueResponse {
            session_id: session_id.to_string(),
            items: records
                .into_iter()
                .map(|record| review_queue_item_response(&record))
                .collect(),
        })
    }

    pub async fn apply_review_decision(
        &mut self,
        session_id: &str,
        request: ApplyReviewDecisionRequest,
    ) -> Result<ApplyReviewDecisionResponse> {
        let _session = self.ensure_session_loaded(session_id)?;
        let mut record = self
            .load_record(session_id, &request.record_id)?
            .with_context(|| format!("unknown review record `{}`", request.record_id))?;

        apply_review_action_to_record(&mut record, &request)?;
        self.persist_review_record(session_id, &record)?;
        self.append_review_event(session_id, &record, &request)?;
        self.ingest_review_feedback(&record, &request);

        Ok(ApplyReviewDecisionResponse {
            session_id: session_id.to_string(),
            item: review_queue_item_response(&record),
        })
    }

    async fn load_or_build_project_ir(
        &mut self,
        session_id: &str,
        include_values: bool,
    ) -> Result<ProjectIr> {
        let session = self.ensure_session_loaded(session_id)?;

        if include_values {
            return build_project_ir_for_session(&session, true).await;
        }

        if let Some(cached) = self.project_ir_cache.get(session_id) {
            return Ok(cached.clone());
        }

        let built = build_project_ir_for_session(&session, false).await?;
        self.project_ir_cache
            .insert(session_id.to_string(), built.clone());
        Ok(built)
    }

    async fn load_or_refresh_project_ir_for_explorer(&mut self, session_id: &str) -> Result<ProjectIr> {
        let session = self.ensure_session_loaded(session_id)?.clone();
        let previous = self.project_ir_cache.get(session_id).cloned();
        let rebuilt = build_project_ir_for_session(&session, false).await?;

        if let Some(cached) = previous {
            if cached != rebuilt {
                self.project_ir_cache
                    .insert(session_id.to_string(), rebuilt.clone());
                self.append_explorer_graph_stale_event(session_id)?;
            }
            return Ok(rebuilt);
        }

        self.project_ir_cache
            .insert(session_id.to_string(), rebuilt.clone());
        Ok(rebuilt)
    }

    fn load_candidate_records(&self, session_id: &str) -> Result<Vec<AuditRecord>> {
        if let Some(store) = &self.session_store {
            return store.list_records(session_id, Some("candidate"));
        }
        Ok(self
            .review_records_cache
            .get(session_id)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|record| matches!(record.kind, AuditRecordKind::Candidate))
            .collect())
    }

    fn load_graph_overlay_records(&mut self, session_id: &str) -> Result<Vec<AuditRecord>> {
        if let Some(cached) = self.graph_overlay_cache.get(session_id) {
            if cached.loaded_at.elapsed() <= GRAPH_OVERLAY_CACHE_TTL {
                return Ok(cached.records.clone());
            }
        }

        let records = if let Some(store) = &self.session_store {
            let mut records = store.list_records(session_id, Some("finding"))?;
            records.extend(store.list_records(session_id, Some("candidate"))?);
            records
        } else {
            self.review_records_cache
                .get(session_id)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter(|record| {
                    matches!(
                        record.kind,
                        AuditRecordKind::Candidate | AuditRecordKind::Finding
                    )
                })
                .collect()
        };

        self.graph_overlay_cache.insert(
            session_id.to_string(),
            GraphOverlayCacheEntry {
                loaded_at: Instant::now(),
                records: records.clone(),
            },
        );

        Ok(records)
    }

    fn load_record(&self, session_id: &str, record_id: &str) -> Result<Option<AuditRecord>> {
        if let Some(store) = &self.session_store {
            return store.load_record(session_id, record_id);
        }
        Ok(self
            .review_records_cache
            .get(session_id)
            .and_then(|records| {
                records
                    .iter()
                    .find(|record| record.record_id == record_id)
                    .cloned()
            }))
    }

    fn persist_review_record(&mut self, session_id: &str, record: &AuditRecord) -> Result<()> {
        if let Some(store) = &self.session_store {
            store.upsert_record(session_id, record)?;
        } else {
            let records = self
                .review_records_cache
                .entry(session_id.to_string())
                .or_default();
            if let Some(position) = records
                .iter()
                .position(|existing| existing.record_id == record.record_id)
            {
                records[position] = record.clone();
            } else {
                records.push(record.clone());
            }
        }
        self.graph_overlay_cache.remove(session_id);
        Ok(())
    }

    fn append_review_event(
        &mut self,
        session_id: &str,
        record: &AuditRecord,
        request: &ApplyReviewDecisionRequest,
    ) -> Result<()> {
        if let Some(store) = &self.session_store {
            let payload = serde_json::json!({
                "recordId": record.record_id,
                "action": review_action_name(&request.action),
                "note": request.note.clone().unwrap_or_default(),
                "labels": record.labels,
                "verificationStatus": verification_status_name(&record.verification_status),
            });
            let event = session_store::SessionEvent {
                event_id: format!(
                    "review-action:{}:{}",
                    session_id,
                    Utc::now().timestamp_micros()
                ),
                event_type: "review.action".to_string(),
                payload: payload.to_string(),
                created_at: Utc::now(),
            };
            store.append_event(session_id, &event)?;
        } else {
            self.review_records_cache
                .entry(session_id.to_string())
                .or_default();
        }
        Ok(())
    }

    fn append_explorer_graph_stale_event(&self, session_id: &str) -> Result<()> {
        let Some(store) = &self.session_store else {
            return Ok(());
        };

        let payload = serde_json::json!({
            "event": "explorer_graph_stale"
        });
        let event = session_store::SessionEvent {
            event_id: format!(
                "explorer-graph-stale:{}:{}",
                session_id,
                Utc::now().timestamp_micros()
            ),
            event_type: "explorer_graph_stale".to_string(),
            payload: payload.to_string(),
            created_at: Utc::now(),
        };
        store.append_event(session_id, &event)?;
        Ok(())
    }

    pub fn append_llm_interaction_event(
        &self,
        session_id: &str,
        provenance: &LlmProvenance,
        succeeded: bool,
    ) -> Result<()> {
        let Some(store) = &self.session_store else {
            return Ok(());
        };
        store.append_llm_interaction_event(
            session_id,
            &LlmInteractionEvent {
                provider: provenance.provider.clone(),
                model: provenance.model.clone(),
                role: provenance.role.clone(),
                duration_ms: provenance.duration_ms,
                prompt_chars: provenance.prompt_chars,
                response_chars: provenance.response_chars,
                attempt: provenance.attempt,
                succeeded,
            },
        )
    }

    pub fn llm_interaction_hook_for_session(&self, session_id: &str) -> LlmInteractionHook {
        let store = self.session_store.clone();
        let session_id = session_id.to_string();
        Arc::new(move |provenance: &LlmProvenance, succeeded: bool| {
            let Some(store) = &store else {
                return;
            };
            let interaction = LlmInteractionEvent {
                provider: provenance.provider.clone(),
                model: provenance.model.clone(),
                role: provenance.role.clone(),
                duration_ms: provenance.duration_ms,
                prompt_chars: provenance.prompt_chars,
                response_chars: provenance.response_chars,
                attempt: provenance.attempt,
                succeeded,
            };
            if let Err(err) = store.append_llm_interaction_event(&session_id, &interaction) {
                tracing::warn!(
                    session_id = %session_id,
                    error = %err,
                    "failed to append llm interaction session event"
                );
            }
        })
    }

    fn ingest_review_feedback(&self, record: &AuditRecord, request: &ApplyReviewDecisionRequest) {
        let store_path = self.knowledge_feedback_store_path();
        let Ok(mut kb) = KnowledgeBase::load_from_repo_root_with_store(&store_path) else {
            return;
        };

        let case = AdjudicatedCase {
            id: record.record_id.clone(),
            title: record.title.clone(),
            summary: record.summary.clone(),
            tags: record.labels.clone(),
        };

        match request.action {
            ReviewDecisionAction::Confirm => kb.ingest_true_positive(case),
            ReviewDecisionAction::Reject => kb.ingest_false_positive(case),
            ReviewDecisionAction::Suppress | ReviewDecisionAction::Annotate => {}
        }

        if let Err(err) = kb.persist_feedback_store(&store_path) {
            eprintln!("{{\"event\":\"knowledge.feedback.persist_failed\",\"error\":\"{err}\"}}");
        }
    }

    async fn seed_default_candidate(&mut self, session_id: &str) -> Result<AuditRecord> {
        let ir = self.load_or_build_project_ir(session_id, false).await?;
        let title = ir
            .security_overview()
            .hotspots
            .into_iter()
            .next()
            .unwrap_or_else(|| {
                "Potential security hotspot requiring analyst validation".to_string()
            });

        let mut record = AuditRecord::candidate(
            format!("cand-{}", Utc::now().timestamp_micros()),
            title,
            VerificationStatus::unverified("AI-assisted triage requires analyst confirmation"),
        );
        record.summary =
            "Generated from project hotspot analysis; validate before promoting to finding."
                .to_string();
        record.labels.push("generated".to_string());
        record.evidence_refs.push("evidence://pending".to_string());
        record.ir_node_ids = default_review_ir_node_ids(&ir);
        self.persist_review_record(session_id, &record)?;
        Ok(record)
    }

    async fn seed_rule_based_candidates(&mut self, session_id: &str) -> Result<Vec<AuditRecord>> {
        let Some(config) = self.audit_config.as_ref() else {
            return Ok(vec![]);
        };

        let rules_dir = crypto_rule_pack_dir()?;
        let evaluator = match RuleEvaluator::load_from_dir(&rules_dir) {
            Ok(value) => value,
            Err(_) => return Ok(vec![]),
        };
        let workspace = match WorkspaceAnalyzer::analyze(&config.source.local_path) {
            Ok(value) => value,
            Err(_) => return Ok(vec![]),
        };
        let engine_ctx = CryptoEngineContext {
            workspace: workspace.clone(),
            build_matrix: config.scope.build_matrix.clone(),
            entry_points: vec![],
            spec_constraints: vec![],
            environment_manifest: EnvironmentManifest {
                rust_toolchain: "rustc unknown".to_string(),
                cargo_lock_hash: "unavailable".to_string(),
                workspace_root: workspace.root.clone(),
                audit_id: config.audit_id.clone(),
                content_hash: config.source.content_hash.clone(),
            },
        };
        let rules_by_id = evaluator
            .rules()
            .iter()
            .map(|rule| (rule.id.clone(), rule.clone()))
            .collect::<HashMap<_, _>>();

        let matches = evaluator.evaluate_workspace(&engine_ctx).await;
        let mut seeded = Vec::<AuditRecord>::new();
        for (idx, matched) in matches.into_iter().enumerate() {
            let Some(rule) = rules_by_id.get(&matched.rule_id) else {
                continue;
            };

            let mut record = AuditRecord::candidate(
                format!("rule-{}-{}", matched.rule_id.to_ascii_lowercase(), idx + 1),
                rule.title.clone(),
                VerificationStatus::unverified(
                    "Deterministic rule match requires analyst confirmation",
                ),
            );
            record.summary = format!(
                "{} Matched snippet: {}",
                rule.description, matched.matched_snippet
            );
            record.severity = Some(rule.severity.clone());
            record.locations.push(matched.location.clone());
            record
                .evidence_refs
                .push(format!("rule://{}", matched.rule_id));
            record.labels.push("deterministic".to_string());
            record
                .labels
                .push(format!("rule:{}", matched.rule_id.to_ascii_lowercase()));
            record.ir_node_ids = matched.ir_node_ids.clone();
            self.persist_review_record(session_id, &record)?;
            seeded.push(record);
        }

        Ok(seeded)
    }

    fn ensure_session_loaded(&mut self, session_id: &str) -> Result<AuditSession> {
        if let Some(existing) = self.sessions.get(session_id) {
            return Ok(existing.clone());
        }

        if let Some(store) = &self.session_store {
            if let Some(session) = store.load_session(session_id)? {
                self.sessions
                    .insert(session_id.to_string(), session.clone());
                return Ok(session);
            }
        }

        Err(crate::UnknownSessionError {
            session_id: session_id.to_string(),
        }
        .into())
    }

    fn ensure_session_exists(&self, session_id: &str) -> Result<()> {
        if self.sessions.contains_key(session_id) {
            return Ok(());
        }

        if let Some(store) = &self.session_store {
            if store.load_session(session_id)?.is_some() {
                return Ok(());
            }
        }

        Err(crate::UnknownSessionError {
            session_id: session_id.to_string(),
        }
        .into())
    }

    fn knowledge_feedback_store_path(&self) -> PathBuf {
        self.work_dir.join("knowledge-feedback.yaml")
    }

    fn memory_block_path(&self) -> Option<PathBuf> {
        if let Ok(raw) = std::env::var("KNOWLEDGE_MEMORY_BLOCK_PATH") {
            let value = raw.trim();
            if !value.is_empty() {
                return Some(PathBuf::from(value));
            }
        }

        let default = self.work_dir.join("knowledge.bin");
        if default.exists() {
            Some(default)
        } else {
            None
        }
    }

    async fn load_research_advisories(&self, session: &AuditSession) -> Vec<ResearchAdvisoryView> {
        let Some(research_service) = &self.research_service else {
            return Vec::new();
        };

        let workspace = match WorkspaceAnalyzer::analyze(&session.snapshot.source.local_path) {
            Ok(workspace) => workspace,
            Err(err) => {
                tracing::warn!(
                    session_id = %session.session_id,
                    error = %err,
                    "failed to analyze workspace for research advisories"
                );
                return Vec::new();
            }
        };

        let mut advisories = Vec::<ResearchAdvisoryView>::new();
        let mut seen_dependencies = HashSet::<String>::new();
        let mut seen_advisories = HashSet::<String>::new();

        'deps: for member in workspace.members {
            for dependency in member.dependencies {
                if !seen_dependencies.insert(dependency.name.clone()) {
                    continue;
                }

                match research_service
                    .query(&ResearchQuery::RustSecAdvisory {
                        crate_name: dependency.name.clone(),
                    })
                    .await
                {
                    Ok(result) => {
                        for finding in result.findings {
                            let key = format!("{}:{}", finding.source, finding.id);
                            if !seen_advisories.insert(key) {
                                continue;
                            }
                            advisories.push(ResearchAdvisoryView {
                                source: finding.source,
                                id: finding.id,
                                title: finding.title,
                                severity: finding.severity,
                                url: finding.url,
                            });
                        }
                    }
                    Err(err) => {
                        if err.to_string().to_ascii_lowercase().contains("rate limit") {
                            break 'deps;
                        }
                    }
                }
            }
        }

        advisories
    }

    fn load_knowledge_base(&self) -> Result<KnowledgeBase> {
        let mut knowledge_base =
            KnowledgeBase::load_from_repo_root_with_store(self.knowledge_feedback_store_path())?;

        if let Some(path) = self.memory_block_path() {
            if path.exists() {
                match resolved_config_and_provider_from_env()
                    .and_then(|(config, embedder)| MemoryBlock::load(&path, &config, embedder))
                {
                    Ok(block) => knowledge_base.attach_memory_block(block),
                    Err(err) => {
                        let payload = serde_json::json!({
                            "event": "knowledge.memory_block.load_failed",
                            "path": path.display().to_string(),
                            "error": err.to_string(),
                        });
                        eprintln!("{payload}");
                    }
                }
            }
        }

        Ok(knowledge_base)
    }
}

impl SourceInputIpc {
    fn into_source_input(self) -> Result<SourceInput> {
        match self.kind {
            SourceKind::Git => {
                let commit = self
                    .commit_or_ref
                    .filter(|value| !value.trim().is_empty())
                    .context("git source requires commitOrRef")?;
                Ok(SourceInput::GitUrl {
                    url: self.value,
                    commit,
                    auth: None,
                    allow_branch_resolution: true,
                })
            }
            SourceKind::Local => Ok(SourceInput::LocalPath {
                path: PathBuf::from(self.value),
                commit: self.commit_or_ref,
            }),
            SourceKind::Archive => Ok(SourceInput::Archive {
                path: PathBuf::from(self.value),
            }),
        }
    }
}

fn make_session_id() -> String {
    format!("sess-{}", Utc::now().timestamp_micros())
}

fn make_snapshot_id() -> String {
    format!("snap-{}", Utc::now().timestamp_micros())
}

fn crypto_rule_pack_dir() -> Result<PathBuf> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir
        .parent()
        .and_then(|value| value.parent())
        .and_then(|value| value.parent())
        .context("resolve repository root from session-manager manifest")?;
    Ok(repo_root.join("data/rules/crypto-misuse"))
}

fn job_to_view(job: &AuditJob) -> SessionJobView {
    SessionJobView {
        job_id: job.job_id.clone(),
        kind: job_kind(&job.kind),
        status: format!("{:?}", job.status).to_ascii_lowercase(),
    }
}

fn job_kind(kind: &AuditJobKind) -> String {
    match kind {
        AuditJobKind::BuildProjectIr => "build_project_ir".to_string(),
        AuditJobKind::GenerateAiOverview => "generate_ai_overview".to_string(),
        AuditJobKind::PlanChecklists => "plan_checklists".to_string(),
        AuditJobKind::RunEngine { engine_name } => format!("run_engine:{engine_name}"),
        AuditJobKind::RunDomainChecklist { domain_id } => {
            format!("run_domain_checklist:{domain_id}")
        }
        AuditJobKind::RunToolAction { action_id } => format!("run_tool_action:{action_id}"),
        AuditJobKind::ExportReports => "export_reports".to_string(),
    }
}

fn session_jobs_from_events(events: &[session_store::SessionEvent]) -> Vec<SessionJobView> {
    let mut seen = HashMap::<String, ()>::new();
    let mut jobs = Vec::<SessionJobView>::new();
    for event in events {
        let Ok(job) = serde_json::from_str::<AuditJob>(&event.payload) else {
            continue;
        };
        if seen.insert(job.job_id.clone(), ()).is_none() {
            jobs.push(job_to_view(&job));
        }
    }
    jobs
}

#[derive(Debug, Deserialize)]
struct LlmInteractionSummaryEvent {
    provider: String,
    role: String,
    duration_ms: u64,
    prompt_chars: usize,
    response_chars: usize,
    succeeded: bool,
}

#[derive(Debug, Deserialize)]
struct ToolActionCompletedEvent {
    tool_family: String,
    status: String,
    duration_ms: u64,
}

#[derive(Debug, Deserialize)]
struct ReviewDecisionEvent {
    action: String,
}

#[derive(Debug, Deserialize)]
struct EngineCompletedEvent {
    engine: String,
    findings_count: usize,
    duration_ms: u64,
}

#[derive(Debug, Deserialize)]
struct EngineFailedEvent {
    engine: String,
}

#[derive(Debug, Default)]
struct LlmRoleAggregate {
    count: usize,
    total_duration_ms: u64,
    total_prompt_chars: usize,
    total_response_chars: usize,
    providers_used: HashSet<String>,
    succeeded: usize,
    failed: usize,
}

#[derive(Debug, Default)]
struct ToolFamilyAggregate {
    count: usize,
    total_duration_ms: u64,
    succeeded: usize,
    failed: usize,
}

fn tool_family_name(tool_family: &ToolFamily) -> &'static str {
    match tool_family {
        ToolFamily::Kani => "kani",
        ToolFamily::Z3 => "z3",
        ToolFamily::CargoFuzz => "cargo_fuzz",
        ToolFamily::MadSim => "madsim",
        ToolFamily::Chaos => "chaos",
        ToolFamily::CircomZ3 => "circom_z3",
        ToolFamily::Research => "research",
        ToolFamily::CairoExternal => "cairo_external",
        ToolFamily::LeanExternal => "lean_external",
    }
}

const MAX_PROJECT_TREE_DEPTH: usize = 7;
const MAX_PROJECT_TREE_NODES: usize = 1_500;
const MAX_SOURCE_FILE_BYTES: u64 = 768 * 1024;

fn canonical_session_root(session: &AuditSession) -> Result<PathBuf> {
    let root = fs::canonicalize(&session.snapshot.source.local_path).with_context(|| {
        format!(
            "session source root is unavailable {}",
            session.snapshot.source.local_path.display()
        )
    })?;

    if root.is_dir() {
        Ok(root)
    } else {
        root.parent()
            .map(Path::to_path_buf)
            .context("session source root is not a directory")
    }
}

fn collect_tree_nodes(
    root_dir: &Path,
    dir: &Path,
    depth: usize,
    remaining: &mut usize,
    scope_roots: Option<&[PathBuf]>,
) -> Result<Vec<ProjectTreeNode>> {
    if depth >= MAX_PROJECT_TREE_DEPTH || *remaining == 0 {
        return Ok(vec![]);
    }

    let mut dirs = Vec::<(String, PathBuf)>::new();
    let mut files = Vec::<(String, PathBuf)>::new();

    let entries =
        fs::read_dir(dir).with_context(|| format!("read project directory {}", dir.display()))?;
    for entry in entries {
        if *remaining == 0 {
            break;
        }

        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if should_skip_tree_entry(&name) {
            continue;
        }

        let file_type = entry.file_type()?;
        let path = entry.path();
        if !tree_entry_in_scope(&path, scope_roots) {
            continue;
        }

        if file_type.is_dir() {
            dirs.push((name, path));
        } else if file_type.is_file() {
            files.push((name, path));
        }
    }

    dirs.sort_by(|a, b| a.0.cmp(&b.0));
    files.sort_by(|a, b| a.0.cmp(&b.0));

    let mut nodes = Vec::<ProjectTreeNode>::new();

    for (name, path) in dirs {
        if *remaining == 0 {
            break;
        }
        *remaining -= 1;

        let relative = path.strip_prefix(root_dir).unwrap_or(path.as_path());
        let children = collect_tree_nodes(root_dir, &path, depth + 1, remaining, scope_roots)?;

        nodes.push(ProjectTreeNode {
            name,
            path: relative_path_to_string(relative),
            kind: ProjectTreeNodeKind::Directory,
            children,
        });
    }

    for (name, path) in files {
        if *remaining == 0 {
            break;
        }
        *remaining -= 1;

        let relative = path.strip_prefix(root_dir).unwrap_or(path.as_path());
        nodes.push(ProjectTreeNode {
            name,
            path: relative_path_to_string(relative),
            kind: ProjectTreeNodeKind::File,
            children: vec![],
        });
    }

    Ok(nodes)
}

fn should_skip_tree_entry(name: &str) -> bool {
    matches!(
        name,
        ".git" | "target" | "node_modules" | ".audit-work" | ".audit-sessions"
    )
}

fn scoped_tree_roots(session: &AuditSession, root_dir: &Path) -> Vec<PathBuf> {
    if session.snapshot.target_crates.is_empty() {
        return vec![];
    }

    let workspace = match WorkspaceAnalyzer::analyze(root_dir) {
        Ok(workspace) => workspace,
        Err(_) => return vec![],
    };

    let targets = session
        .snapshot
        .target_crates
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();

    workspace
        .members
        .into_iter()
        .filter_map(|member| {
            if !targets.contains(member.name.as_str()) {
                return None;
            }
            fs::canonicalize(member.path).ok()
        })
        .filter(|path| path.starts_with(root_dir))
        .collect()
}

fn tree_entry_in_scope(path: &Path, scope_roots: Option<&[PathBuf]>) -> bool {
    let Some(scope_roots) = scope_roots else {
        return true;
    };

    scope_roots
        .iter()
        .any(|scope_root| path.starts_with(scope_root) || scope_root.starts_with(path))
}

fn normalized_relative_path(input: &str) -> Result<PathBuf> {
    let path = PathBuf::from(input.trim());
    if path.as_os_str().is_empty() {
        bail!("path must not be empty");
    }

    if path.is_absolute() {
        bail!("absolute paths are not allowed");
    }

    for component in path.components() {
        use std::path::Component;
        match component {
            Component::Prefix(_) | Component::RootDir | Component::ParentDir => {
                bail!("path traversal is not allowed")
            }
            Component::CurDir | Component::Normal(_) => {}
        }
    }

    Ok(path)
}

fn relative_path_to_string(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn console_entry_from_event(event: &session_store::SessionEvent) -> SessionConsoleEntry {
    let mut message = event.event_type.clone();
    let mut level = SessionConsoleLevel::Info;

    if event.event_type == "job.lifecycle" {
        if let Ok(job) = serde_json::from_str::<AuditJob>(&event.payload) {
            message = format!(
                "{} [{}] {}",
                job.job_id,
                format!("{:?}", job.status).to_ascii_lowercase(),
                job_kind(&job.kind)
            );
            if job.status == orchestrator::AuditJobStatus::Failed {
                level = SessionConsoleLevel::Error;
            }
        }
    }
    if event.event_type == "provider.failover" {
        level = SessionConsoleLevel::Warning;
        if let Ok(orchestrator::AuditEvent::ProviderFailover { from, to, role, .. }) =
            serde_json::from_str::<orchestrator::AuditEvent>(&event.payload)
        {
            message = format!("{role} failover: {from} -> {to}");
        }
    }
    if event.event_type == "adviser.consulted" {
        if let Ok(orchestrator::AuditEvent::AdviserConsulted {
            engine,
            suggestion,
            applied,
        }) = serde_json::from_str::<orchestrator::AuditEvent>(&event.payload)
        {
            let status = if applied { "applied" } else { "observed" };
            message = format!("adviser {status} for {engine}: {suggestion}");
        }
    }

    SessionConsoleEntry {
        timestamp: event.created_at.format("%H:%M:%S").to_string(),
        source: event.event_type.clone(),
        level,
        message,
    }
}

async fn build_project_ir_for_session(
    session: &AuditSession,
    include_values: bool,
) -> Result<ProjectIr> {
    ProjectIrBuilder::for_path(&session.snapshot.source.local_path)
        .with_value_previews(include_values)
        .build()
        .await
        .with_context(|| {
            format!(
                "build project ir for session {} ({})",
                session.session_id,
                session.snapshot.source.local_path.display()
            )
        })
}

fn map_project_ir_build_error(err: anyhow::Error) -> anyhow::Error {
    let has_not_found_io = err.chain().any(|cause| {
        cause
            .downcast_ref::<std::io::Error>()
            .map(|io_error| io_error.kind() == std::io::ErrorKind::NotFound)
            .unwrap_or(false)
    });

    let message = err.to_string().to_ascii_lowercase();
    let missing_source_context = message.contains("path does not exist")
        || message.contains("resolve_source must be called")
        || message.contains("confirm_workspace must be called");

    if has_not_found_io || missing_source_context {
        anyhow!("ProjectIR has not been built for this session. Run BuildProjectIr first.")
    } else {
        err
    }
}

fn file_graph_response(session_id: &str, ir: &ProjectIr) -> ProjectGraphResponse {
    ProjectGraphResponse {
        session_id: session_id.to_string(),
        lens: "file".to_string(),
        redacted_values: true,
        nodes: ir
            .file_graph
            .nodes
            .iter()
            .map(|node| ProjectGraphNodeResponse {
                id: node.id.clone(),
                label: node.path.display().to_string(),
                kind: format!("file:{}", node.language),
                file_path: Some(relative_path_to_string(&node.path)),
                line: None,
                finding_count: None,
                max_severity: None,
            })
            .collect(),
        edges: ir
            .file_graph
            .edges
            .iter()
            .map(|edge| ProjectGraphEdgeResponse {
                from: edge.from.clone(),
                to: edge.to.clone(),
                relation: edge.relation.clone(),
                value_preview: None,
            })
            .collect(),
    }
}

fn feature_graph_response(session_id: &str, ir: &ProjectIr) -> ProjectGraphResponse {
    ProjectGraphResponse {
        session_id: session_id.to_string(),
        lens: "feature".to_string(),
        redacted_values: true,
        nodes: ir
            .feature_graph
            .nodes
            .iter()
            .map(|node| ProjectGraphNodeResponse {
                id: node.id.clone(),
                label: node.name.clone(),
                kind: "feature".to_string(),
                file_path: Some(node.source.clone()),
                line: None,
                finding_count: None,
                max_severity: None,
            })
            .collect(),
        edges: ir
            .feature_graph
            .edges
            .iter()
            .map(|edge| ProjectGraphEdgeResponse {
                from: edge.from.clone(),
                to: edge.to.clone(),
                relation: edge.relation.clone(),
                value_preview: None,
            })
            .collect(),
    }
}

fn dataflow_graph_response(
    session_id: &str,
    ir: &ProjectIr,
    redacted_values: bool,
) -> ProjectGraphResponse {
    ProjectGraphResponse {
        session_id: session_id.to_string(),
        lens: "dataflow".to_string(),
        redacted_values,
        nodes: ir
            .dataflow_graph
            .nodes
            .iter()
            .map(|node| ProjectGraphNodeResponse {
                id: node.id.clone(),
                label: node.label.clone(),
                kind: "dataflow".to_string(),
                file_path: node.file.as_ref().map(|path| relative_path_to_string(path)),
                line: None,
                finding_count: None,
                max_severity: None,
            })
            .collect(),
        edges: ir
            .dataflow_graph
            .edges
            .iter()
            .map(|edge| ProjectGraphEdgeResponse {
                from: edge.from.clone(),
                to: edge.to.clone(),
                relation: edge.relation.clone(),
                value_preview: edge.value_preview.clone(),
            })
            .collect(),
    }
}

fn symbol_graph_response(session_id: &str, ir: &ProjectIr) -> ProjectGraphResponse {
    ProjectGraphResponse {
        session_id: session_id.to_string(),
        lens: "symbol".to_string(),
        redacted_values: true,
        nodes: ir
            .symbol_graph
            .nodes
            .iter()
            .map(|node| ProjectGraphNodeResponse {
                id: node.id.clone(),
                label: node.name.clone(),
                kind: node.kind.clone(),
                file_path: Some(relative_path_to_string(&node.file)),
                line: Some(node.line),
                finding_count: None,
                max_severity: None,
            })
            .collect(),
        edges: ir
            .symbol_graph
            .edges
            .iter()
            .map(|edge| ProjectGraphEdgeResponse {
                from: edge.from.clone(),
                to: edge.to.clone(),
                relation: edge.relation.clone(),
                value_preview: None,
            })
            .collect(),
    }
}

fn normalized_graph_path(path: &str) -> String {
    path.trim()
        .replace('\\', "/")
        .trim_start_matches("./")
        .to_string()
}

fn is_absolute_like(path: &str) -> bool {
    Path::new(path).is_absolute()
        || path
            .as_bytes()
            .get(1)
            .map(|byte| *byte == b':')
            .unwrap_or(false)
}

fn split_path_segments(path: &str) -> Vec<&str> {
    path.split('/')
        .filter(|segment| !segment.is_empty())
        .collect()
}

fn has_segment_suffix(path: &str, suffix: &str) -> bool {
    let path_segments = split_path_segments(path);
    let suffix_segments = split_path_segments(suffix);
    if suffix_segments.is_empty() || suffix_segments.len() > path_segments.len() {
        return false;
    }

    let start = path_segments.len() - suffix_segments.len();
    path_segments[start..] == suffix_segments
}

fn graph_paths_match(node_path: &str, record_path: &str) -> bool {
    if node_path == record_path {
        return true;
    }

    let node_absolute = is_absolute_like(node_path);
    let record_absolute = is_absolute_like(record_path);
    if node_absolute == record_absolute {
        return false;
    }

    if node_absolute {
        has_segment_suffix(node_path, record_path)
    } else {
        has_segment_suffix(record_path, node_path)
    }
}

fn normalize_record_location_path(path: &Path, source_root: Option<&Path>) -> String {
    if let Some(root) = source_root {
        if let Ok(stripped) = path.strip_prefix(root) {
            return normalized_graph_path(&stripped.to_string_lossy());
        }
    }

    normalized_graph_path(&path.to_string_lossy())
}

fn severity_rank(severity: &Severity) -> u8 {
    match severity {
        Severity::Critical => 5,
        Severity::High => 4,
        Severity::Medium => 3,
        Severity::Low => 2,
        Severity::Observation => 1,
    }
}

fn annotate_graph_with_findings(
    nodes: &mut [ProjectGraphNodeResponse],
    records: &[AuditRecord],
    source_root: Option<&Path>,
) {
    for node in nodes.iter_mut() {
        node.finding_count = None;
        node.max_severity = None;

        let Some(file_path) = node.file_path.as_ref() else {
            continue;
        };
        let normalized = normalized_graph_path(file_path);
        if normalized.is_empty() {
            continue;
        }

        let mut count = 0_u32;
        let mut max_severity: Option<Severity> = None;

        for record in records {
            let record_matches = record.locations.iter().any(|location| {
                let path = normalize_record_location_path(&location.file, source_root);
                !path.is_empty() && graph_paths_match(&normalized, &path)
            });

            if !record_matches {
                continue;
            }

            count = count.saturating_add(1);
            if let Some(severity) = record.severity.as_ref() {
                let is_higher = max_severity
                    .as_ref()
                    .map(|current| severity_rank(severity) > severity_rank(current))
                    .unwrap_or(true);
                if is_higher {
                    max_severity = Some(severity.clone());
                }
            }
        }

        if count > 0 {
            node.finding_count = Some(count);
        }
        if let Some(severity) = max_severity.as_ref() {
            node.max_severity = Some(severity_name(severity).to_string());
        }
    }
}

fn security_overview_response(
    session_id: &str,
    overview: IrSecurityOverview,
) -> LoadSecurityOverviewResponse {
    LoadSecurityOverviewResponse {
        session_id: session_id.to_string(),
        assets: overview.assets,
        trust_boundaries: overview.trust_boundaries,
        hotspots: overview.hotspots,
        review_notes: overview.review_notes,
    }
}

fn prepend_coverage_warnings(
    review_notes: Vec<String>,
    coverage: Option<&CoverageReport>,
) -> Vec<String> {
    let mut prefixed = coverage
        .map(|coverage| {
            coverage
                .warnings
                .iter()
                .map(|warning| format!("[COVERAGE] {warning}"))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if prefixed.is_empty() {
        return review_notes;
    }
    prefixed.extend(review_notes);
    prefixed
}

fn checklist_plan_response(session_id: &str, plan: IrChecklistPlan) -> LoadChecklistPlanResponse {
    LoadChecklistPlanResponse {
        session_id: session_id.to_string(),
        domains: checklist_domain_responses(&plan),
    }
}

fn checklist_domain_responses(plan: &IrChecklistPlan) -> Vec<ChecklistDomainPlanResponse> {
    plan.domains
        .iter()
        .map(|domain| ChecklistDomainPlanResponse {
            id: domain.id.clone(),
            rationale: domain.rationale.clone(),
        })
        .collect()
}

fn toolbench_context_terms(
    selection: &ToolbenchSelectionRequest,
    checklist: &IrChecklistPlan,
    overview: &IrSecurityOverview,
    ir: &ProjectIr,
) -> Vec<String> {
    let mut terms = Vec::<String>::new();
    let mut seen = HashSet::<String>::new();

    let mut push_term = |value: &str| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return;
        }
        let normalized = trimmed.to_ascii_lowercase();
        if seen.insert(normalized.clone()) {
            terms.push(normalized);
        }
    };

    push_term(&selection.kind);
    push_term(&selection.id);
    push_term(&format!("{}:{}", selection.kind, selection.id));

    for domain in &checklist.domains {
        push_term(&domain.id);
        for token in split_context_tokens(&domain.rationale) {
            push_term(&token);
        }

        match domain.id.as_str() {
            "crypto" => {
                push_term("rust");
                push_term("cargo");
            }
            "zk" => {
                push_term("circom");
                push_term("cairo");
                push_term("starknet");
            }
            "p2p-consensus" => {
                push_term("p2p");
                push_term("consensus");
                push_term("distributed");
            }
            _ => {}
        }
    }

    for note in &overview.review_notes {
        for token in split_context_tokens(note) {
            push_term(&token);
        }
    }

    let seed_ids = toolbench_seed_node_ids(ir, selection);
    if !seed_ids.is_empty() {
        let neighborhood = ir.ir_neighborhood(&seed_ids, 24, 2);
        for node_id in &neighborhood {
            push_term(node_id);
        }

        let subgraph = ir.subgraph_for_nodes(&neighborhood);
        for node in &subgraph.file_graph.nodes {
            push_term(&relative_path_to_string(&node.path));
            push_term(&node.language);
        }
        for node in &subgraph.symbol_graph.nodes {
            push_term(&node.kind);
            for token in split_context_tokens(&node.name) {
                push_term(&token);
            }
        }
        for node in &subgraph.feature_graph.nodes {
            push_term(&node.name);
            for token in split_context_tokens(&node.source) {
                push_term(&token);
            }
        }
        for edge in &subgraph.dataflow_graph.edges {
            push_term(&edge.relation);
        }
        for snippet in ir.context_snippets_for_nodes(&neighborhood, 900) {
            push_term(&snippet.node_id);
            push_term(&relative_path_to_string(&snippet.file_path));
            for token in split_context_tokens(&snippet.snippet).into_iter().take(24) {
                push_term(&token);
            }
        }
    }

    terms
}

fn toolbench_seed_node_ids(ir: &ProjectIr, selection: &ToolbenchSelectionRequest) -> Vec<String> {
    let mut seeds = Vec::<String>::new();
    match selection.kind.as_str() {
        "file" => {
            for node in &ir.file_graph.nodes {
                let relative = relative_path_to_string(&node.path);
                let absolute = node.path.to_string_lossy();
                if selection.id == node.id
                    || selection.id == relative
                    || selection.id == absolute
                    || absolute.ends_with(&selection.id)
                {
                    seeds.push(node.id.clone());
                }
            }
        }
        "symbol" => {
            let query = selection.id.trim().to_ascii_lowercase();
            for node in &ir.symbol_graph.nodes {
                if selection.id == node.id
                    || node.name.eq_ignore_ascii_case(&selection.id)
                    || (!query.is_empty() && node.name.to_ascii_lowercase().contains(&query))
                {
                    seeds.push(node.id.clone());
                }
            }
        }
        "session" => {}
        _ => {}
    }

    seeds.sort();
    seeds.dedup();
    seeds
}

fn split_context_tokens(value: &str) -> Vec<String> {
    value
        .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '-' && ch != '_')
        .map(|token| token.trim())
        .filter(|token| token.len() >= 3)
        .map(|token| token.to_ascii_lowercase())
        .collect()
}

fn canonical_tool_id(tool: &str) -> String {
    match tool.trim().to_ascii_lowercase().as_str() {
        "kani" => "Kani".to_string(),
        "z3" => "Z3".to_string(),
        "cargo-fuzz" | "cargo_fuzz" | "cargofuzz" => "Cargo Fuzz".to_string(),
        "madsim" => "MadSim".to_string(),
        "chaos" | "chaos-replay" | "chaos_replay" => "Chaos".to_string(),
        "circom-z3" | "circom_z3" | "circom z3" | "circom-graph" => "Circom Z3".to_string(),
        "cairo-external" | "cairo_external" | "cairo-graph" => "Cairo External".to_string(),
        "lean-external" | "lean_external" => "Lean External".to_string(),
        "" => "Kani".to_string(),
        _ => tool.trim().to_string(),
    }
}

fn push_recommendation(
    recommendations: &mut Vec<ToolbenchRecommendationResponse>,
    tool_id: String,
    rationale: String,
) {
    if let Some(existing) = recommendations
        .iter_mut()
        .find(|entry| entry.tool_id == tool_id)
    {
        if !existing.rationale.contains(&rationale) {
            existing.rationale = format!("{} {}", existing.rationale, rationale);
        }
        return;
    }

    recommendations.push(ToolbenchRecommendationResponse { tool_id, rationale });
}

fn fallback_tool_recommendations(plan: &IrChecklistPlan) -> Vec<ToolbenchRecommendationResponse> {
    let mut recommendations = Vec::<ToolbenchRecommendationResponse>::new();

    for domain in &plan.domains {
        match domain.id.as_str() {
            "crypto" => {
                push_recommendation(
                    &mut recommendations,
                    "Kani".to_string(),
                    format!("Recommended by {} checklist.", domain.id),
                );
                push_recommendation(
                    &mut recommendations,
                    "Z3".to_string(),
                    format!("Constraint checks aligned with {} rationale.", domain.id),
                );
                push_recommendation(
                    &mut recommendations,
                    "Cargo Fuzz".to_string(),
                    format!("Input mutation coverage requested by {} scope.", domain.id),
                );
            }
            "zk" => {
                push_recommendation(
                    &mut recommendations,
                    "Circom Z3".to_string(),
                    format!("ZK checklist selected: {}", domain.rationale),
                );
                push_recommendation(
                    &mut recommendations,
                    "Z3".to_string(),
                    "SMT proving flow supports zk invariants.".to_string(),
                );
            }
            "p2p-consensus" => {
                push_recommendation(
                    &mut recommendations,
                    "MadSim".to_string(),
                    format!("Scenario simulation suggested by {} checklist.", domain.id),
                );
                push_recommendation(
                    &mut recommendations,
                    "Chaos".to_string(),
                    format!(
                        "Fault-injection testing suggested by {} checklist.",
                        domain.id
                    ),
                );
            }
            _ => {}
        }
    }

    if recommendations.is_empty() {
        push_recommendation(
            &mut recommendations,
            "Kani".to_string(),
            "Fallback deterministic baseline for unresolved selection.".to_string(),
        );
    }

    recommendations
}

fn generate_audit_plan(
    session: &AuditSession,
    overview: &IrSecurityOverview,
    checklist_plan: &IrChecklistPlan,
    tool_recommendations: &[ToolbenchRecommendationResponse],
    config: &AuditConfig,
) -> AuditPlan {
    AuditPlan {
        plan_id: format!("plan-{}", Utc::now().timestamp_micros()),
        session_id: session.session_id.clone(),
        overview: AuditPlanOverview {
            assets: overview.assets.clone(),
            trust_boundaries: overview.trust_boundaries.clone(),
            hotspots: overview.hotspots.clone(),
        },
        domains: checklist_plan
            .domains
            .iter()
            .map(|domain| AuditPlanDomain {
                id: domain.id.clone(),
                rationale: domain.rationale.clone(),
            })
            .collect(),
        recommended_tools: tool_recommendations
            .iter()
            .map(|tool| AuditPlanTool {
                tool: tool.tool_id.clone(),
                rationale: tool.rationale.clone(),
            })
            .collect(),
        engines: AuditPlanEngines {
            crypto_zk: config.engines.crypto_zk,
            distributed: config.engines.distributed,
        },
        rationale: format!(
            "Generated from workspace analysis of {} target crates with {} detected frameworks.",
            config.scope.target_crates.len(),
            config.scope.detected_frameworks.len(),
        ),
        created_at: Utc::now(),
    }
}

fn default_review_ir_node_ids(ir: &ProjectIr) -> Vec<String> {
    let mut ids = Vec::<String>::new();
    let mut seen = HashSet::<String>::new();

    let push_id = |id: &str, ids: &mut Vec<String>, seen: &mut HashSet<String>| {
        if !id.trim().is_empty() && seen.insert(id.to_string()) {
            ids.push(id.to_string());
        }
    };

    let mut sorted_edges = ir.dataflow_graph.edges.iter().collect::<Vec<_>>();
    sorted_edges.sort_by(|a, b| {
        (a.from.as_str(), a.to.as_str(), a.relation.as_str()).cmp(&(
            b.from.as_str(),
            b.to.as_str(),
            b.relation.as_str(),
        ))
    });

    for edge in sorted_edges.into_iter().take(2) {
        push_id(&edge.from, &mut ids, &mut seen);
        push_id(&edge.to, &mut ids, &mut seen);
    }

    if ids.is_empty() {
        let mut sorted_symbol_ids = ir
            .symbol_graph
            .nodes
            .iter()
            .map(|node| node.id.as_str())
            .collect::<Vec<_>>();
        sorted_symbol_ids.sort_unstable();
        for symbol_id in sorted_symbol_ids.into_iter().take(3) {
            push_id(symbol_id, &mut ids, &mut seen);
        }
    }

    if ids.is_empty() {
        let mut sorted_file_ids = ir
            .file_graph
            .nodes
            .iter()
            .map(|node| node.id.as_str())
            .collect::<Vec<_>>();
        sorted_file_ids.sort_unstable();
        for file_id in sorted_file_ids.into_iter().take(3) {
            push_id(file_id, &mut ids, &mut seen);
        }
    }

    ids
}

fn review_queue_item_response(record: &AuditRecord) -> ReviewQueueItemResponse {
    ReviewQueueItemResponse {
        record_id: record.record_id.clone(),
        kind: match record.kind {
            AuditRecordKind::ReviewNote => "review_note".to_string(),
            AuditRecordKind::Candidate => "candidate".to_string(),
            AuditRecordKind::Finding => "finding".to_string(),
        },
        title: record.title.clone(),
        summary: record.summary.clone(),
        severity: record
            .severity
            .as_ref()
            .map(severity_name)
            .map(str::to_string),
        verification_status: verification_status_name(&record.verification_status).to_string(),
        labels: record.labels.clone(),
        evidence_refs: record.evidence_refs.clone(),
        ir_node_ids: record.ir_node_ids.clone(),
    }
}

fn apply_review_action_to_record(
    record: &mut AuditRecord,
    request: &ApplyReviewDecisionRequest,
) -> Result<()> {
    if let Some(note) = request
        .note
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        record.summary = format!("{} Note: {}", record.summary, note);
    }

    match request.action {
        ReviewDecisionAction::Confirm => {
            if record.evidence_refs.is_empty() {
                bail!("confirm action requires at least one evidence reference");
            }
            record.kind = AuditRecordKind::Finding;
            record.verification_status = VerificationStatus::Verified;
            record.severity.get_or_insert(Severity::Medium);
            push_label(&mut record.labels, "confirmed");
            push_label(&mut record.labels, "verified");
        }
        ReviewDecisionAction::Reject => {
            record.kind = AuditRecordKind::Candidate;
            record.verification_status =
                VerificationStatus::unverified("Marked false positive by analyst");
            push_label(&mut record.labels, "rejected");
            push_label(&mut record.labels, "false-positive");
        }
        ReviewDecisionAction::Suppress => {
            push_label(&mut record.labels, "suppressed");
        }
        ReviewDecisionAction::Annotate => {
            push_label(&mut record.labels, "annotated");
        }
    }

    Ok(())
}

fn push_label(labels: &mut Vec<String>, value: &str) {
    if labels.iter().any(|existing| existing == value) {
        return;
    }
    labels.push(value.to_string());
}

fn review_action_name(action: &ReviewDecisionAction) -> &'static str {
    match action {
        ReviewDecisionAction::Confirm => "confirm",
        ReviewDecisionAction::Reject => "reject",
        ReviewDecisionAction::Suppress => "suppress",
        ReviewDecisionAction::Annotate => "annotate",
    }
}

fn verification_status_name(status: &VerificationStatus) -> &'static str {
    match status {
        VerificationStatus::Verified => "verified",
        VerificationStatus::Unverified { .. } => "unverified",
    }
}

fn severity_name(severity: &Severity) -> &'static str {
    match severity {
        Severity::Critical => "critical",
        Severity::High => "high",
        Severity::Medium => "medium",
        Severity::Low => "low",
        Severity::Observation => "observation",
    }
}

fn test_audit_config(work_dir: &Path) -> AuditConfig {
    AuditConfig {
        audit_id: format!("audit-test-{}", Utc::now().timestamp()),
        source: ResolvedSource {
            local_path: work_dir.to_path_buf(),
            origin: SourceOrigin::Local {
                original_path: work_dir.to_path_buf(),
            },
            commit_hash: "0000000000000000000000000000000000000000".to_string(),
            content_hash: "sha256:test".to_string(),
        },
        scope: audit_agent_core::audit_config::ResolvedScope {
            target_crates: vec![],
            excluded_crates: vec![],
            build_matrix: vec![],
            detected_frameworks: vec![],
        },
        engines: audit_agent_core::audit_config::EngineConfig {
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
        optional_inputs: audit_agent_core::audit_config::OptionalInputs {
            spec_document: None,
            previous_audit: None,
            custom_invariants: vec![],
            known_entry_points: vec![],
        },
        llm: audit_agent_core::audit_config::LlmConfig {
            api_key_present: false,
            provider: None,
            no_llm_prose: false,
            roles: std::collections::HashMap::new(),
        },
        output_dir: PathBuf::from("audit-output"),
    }
}

fn session_orchestrator(
    work_dir: &Path,
    session_store: Option<Arc<SessionStore>>,
) -> AuditOrchestrator {
    let mut orchestrator = AuditOrchestrator::new(
        work_dir.join("audit-output"),
        work_dir.join("evidence-pack.zip"),
    );
    if let Some(store) = session_store {
        orchestrator = orchestrator.with_session_store(store);
    }
    orchestrator
}

fn default_validated_config(source: &ResolvedSource) -> ValidatedConfig {
    let source = match &source.origin {
        SourceOrigin::Git {
            url,
            original_ref: _,
        } => RawSource {
            url: Some(url.clone()),
            local_path: None,
            commit: Some(source.commit_hash.clone()),
        },
        SourceOrigin::Local { original_path } => RawSource {
            url: None,
            local_path: Some(original_path.display().to_string()),
            commit: Some(source.commit_hash.clone()),
        },
        SourceOrigin::Archive {
            original_filename: _,
        } => RawSource {
            url: None,
            local_path: Some(source.local_path.display().to_string()),
            commit: Some(source.commit_hash.clone()),
        },
    };

    ValidatedConfig {
        source,
        scope: RawScope {
            target_crates: None,
            exclude_crates: None,
            features: None,
        },
        engines: RawEngineConfig {
            crypto_zk: Some(true),
            distributed: Some(false),
        },
        budget: BudgetConfig {
            kani_timeout_secs: 300,
            z3_timeout_secs: 600,
            fuzz_duration_secs: 3600,
            madsim_ticks: 100_000,
            max_llm_retries: 3,
            semantic_index_timeout_secs: 120,
        },
        llm: RawLlmConfig::default(),
        output_dir: PathBuf::from("audit-output"),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::fs;

    use super::*;

    #[test]
    fn confirm_action_requires_evidence_refs() {
        let mut record = AuditRecord::candidate(
            "cand-1",
            "candidate finding",
            VerificationStatus::unverified("pending"),
        );
        let request = ApplyReviewDecisionRequest {
            record_id: "cand-1".to_string(),
            action: ReviewDecisionAction::Confirm,
            note: None,
        };

        let err = apply_review_action_to_record(&mut record, &request)
            .expect_err("confirm without evidence should fail");
        assert!(err.to_string().contains("evidence"));
        assert!(matches!(
            record.verification_status,
            VerificationStatus::Unverified { .. }
        ));
    }

    #[test]
    fn confirm_action_sets_verified_when_evidence_exists() {
        let mut record = AuditRecord::candidate(
            "cand-2",
            "candidate finding",
            VerificationStatus::unverified("pending"),
        );
        record.evidence_refs.push("evidence://tool-run".to_string());
        let request = ApplyReviewDecisionRequest {
            record_id: "cand-2".to_string(),
            action: ReviewDecisionAction::Confirm,
            note: None,
        };

        apply_review_action_to_record(&mut record, &request)
            .expect("confirm with evidence should succeed");
        assert!(matches!(
            record.verification_status,
            VerificationStatus::Verified
        ));
    }

    #[test]
    fn default_review_ir_node_ids_are_deterministic_for_same_edge_set() {
        let mut ir_a = ProjectIr::default();
        ir_a.dataflow_graph.edges.push(project_ir::DataflowEdge {
            from: "dataflow:Z".to_string(),
            to: "dataflow:A".to_string(),
            relation: "x".to_string(),
            value_preview: None,
        });
        ir_a.dataflow_graph.edges.push(project_ir::DataflowEdge {
            from: "dataflow:B".to_string(),
            to: "dataflow:C".to_string(),
            relation: "x".to_string(),
            value_preview: None,
        });

        let mut ir_b = ProjectIr::default();
        ir_b.dataflow_graph.edges.push(project_ir::DataflowEdge {
            from: "dataflow:B".to_string(),
            to: "dataflow:C".to_string(),
            relation: "x".to_string(),
            value_preview: None,
        });
        ir_b.dataflow_graph.edges.push(project_ir::DataflowEdge {
            from: "dataflow:Z".to_string(),
            to: "dataflow:A".to_string(),
            relation: "x".to_string(),
            value_preview: None,
        });

        let ids_a = default_review_ir_node_ids(&ir_a);
        let ids_b = default_review_ir_node_ids(&ir_b);
        assert_eq!(
            ids_a, ids_b,
            "provenance seed ordering must be stable for identical edge sets"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn get_project_tree_respects_target_crate_scope() {
        let workspace = tempfile::tempdir().expect("tempdir");
        fs::write(
            workspace.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"crate-a\", \"crate-b\"]\nresolver = \"2\"\n",
        )
        .expect("write workspace manifest");
        fs::create_dir_all(workspace.path().join("crate-a/src")).expect("create crate-a src");
        fs::create_dir_all(workspace.path().join("crate-b/src")).expect("create crate-b src");
        fs::write(
            workspace.path().join("crate-a/Cargo.toml"),
            "[package]\nname = \"crate-a\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .expect("write crate-a manifest");
        fs::write(
            workspace.path().join("crate-b/Cargo.toml"),
            "[package]\nname = \"crate-b\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .expect("write crate-b manifest");
        fs::write(
            workspace.path().join("crate-a/src/lib.rs"),
            "pub fn a() {}\n",
        )
        .expect("write crate-a source");
        fs::write(
            workspace.path().join("crate-b/src/lib.rs"),
            "pub fn b() {}\n",
        )
        .expect("write crate-b source");

        let mut state = UiSessionState::new(workspace.path().join(".audit-work"));
        let mut config = test_audit_config(workspace.path());
        config.source.local_path = workspace.path().to_path_buf();
        config.scope.target_crates = vec!["crate-a".to_string()];
        config.scope.excluded_crates = vec!["crate-b".to_string()];

        let session = AuditSession {
            session_id: "sess-scope".to_string(),
            snapshot: project_snapshot_from_config(&config, "snap-scope".to_string()),
            selected_domains: vec![],
            ui_state: SessionUiState::default(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        state.sessions.insert(session.session_id.clone(), session);

        let tree = state
            .get_project_tree("sess-scope")
            .await
            .expect("project tree");
        let paths = all_tree_paths(&tree.nodes);

        assert!(paths.contains("crate-a"), "expected crate-a in scoped tree");
        assert!(
            !paths.contains("crate-b"),
            "excluded crate-b should not appear in project tree"
        );
    }

    fn all_tree_paths(nodes: &[ProjectTreeNode]) -> HashSet<String> {
        let mut paths = HashSet::<String>::new();
        for node in nodes {
            paths.insert(node.path.clone());
            paths.extend(all_tree_paths(&node.children));
        }
        paths
    }

    #[test]
    fn coverage_warnings_are_prefixed_and_prepended_to_review_notes() {
        let existing = vec![
            "AI-generated overview material must remain unverified until analyst review"
                .to_string(),
        ];
        let coverage = audit_agent_core::output::CoverageReport {
            engines_requested: 2,
            engines_completed: 1,
            engines_failed: 1,
            engines_skipped: 0,
            coverage_complete: false,
            warnings: vec!["Engine 'crypto_zk' failed: timeout".to_string()],
            failover_warnings: vec![],
        };

        let merged = prepend_coverage_warnings(existing.clone(), Some(&coverage));

        assert_eq!(
            merged.first(),
            Some(&"[COVERAGE] Engine 'crypto_zk' failed: timeout".to_string())
        );
        assert_eq!(merged[1], existing[0]);
    }

    #[test]
    fn annotate_graph_with_findings_populates_count_and_max_severity() {
        let mut nodes = vec![
            ProjectGraphNodeResponse {
                id: "node-a".to_string(),
                label: "lib.rs".to_string(),
                kind: "file".to_string(),
                file_path: Some("src/lib.rs".to_string()),
                line: None,
                finding_count: None,
                max_severity: None,
            },
            ProjectGraphNodeResponse {
                id: "node-b".to_string(),
                label: "mod.rs".to_string(),
                kind: "file".to_string(),
                file_path: Some("src/mod.rs".to_string()),
                line: None,
                finding_count: None,
                max_severity: None,
            },
            ProjectGraphNodeResponse {
                id: "node-c".to_string(),
                label: "summary".to_string(),
                kind: "feature".to_string(),
                file_path: Some("src/summary.rs".to_string()),
                line: None,
                finding_count: None,
                max_severity: None,
            },
        ];

        let mut finding = AuditRecord::candidate(
            "finding-1",
            "Critical issue",
            VerificationStatus::unverified("pending"),
        );
        finding.kind = AuditRecordKind::Finding;
        finding.severity = Some(Severity::Critical);
        finding
            .locations
            .push(audit_agent_core::finding::CodeLocation {
                crate_name: "core".to_string(),
                module: "core::lib".to_string(),
                file: PathBuf::from("/tmp/repo/src/lib.rs"),
                line_range: (10, 12),
                snippet: None,
            });

        let mut candidate = AuditRecord::candidate(
            "candidate-1",
            "Low issue",
            VerificationStatus::unverified("pending"),
        );
        candidate.severity = Some(Severity::Low);
        candidate
            .locations
            .push(audit_agent_core::finding::CodeLocation {
                crate_name: "core".to_string(),
                module: "core::mod".to_string(),
                file: PathBuf::from("src/lib.rs"),
                line_range: (20, 22),
                snippet: None,
            });

        let mut unmatched = AuditRecord::candidate(
            "candidate-2",
            "Observation",
            VerificationStatus::unverified("pending"),
        );
        unmatched.severity = Some(Severity::Observation);
        unmatched
            .locations
            .push(audit_agent_core::finding::CodeLocation {
                crate_name: "core".to_string(),
                module: "core::other".to_string(),
                file: PathBuf::from("src/other.rs"),
                line_range: (1, 1),
                snippet: None,
            });

        annotate_graph_with_findings(&mut nodes, &[finding, candidate, unmatched], None);

        assert_eq!(nodes[0].finding_count, Some(2));
        assert_eq!(nodes[0].max_severity.as_deref(), Some("critical"));
        assert_eq!(nodes[1].finding_count, None);
        assert_eq!(nodes[1].max_severity, None);
        assert_eq!(nodes[2].finding_count, None);
        assert_eq!(nodes[2].max_severity, None);
    }

    #[test]
    fn annotate_graph_with_findings_does_not_match_same_suffix_from_other_crates() {
        let mut nodes = vec![ProjectGraphNodeResponse {
            id: "node-a".to_string(),
            label: "src/lib.rs".to_string(),
            kind: "file".to_string(),
            file_path: Some("src/lib.rs".to_string()),
            line: None,
            finding_count: None,
            max_severity: None,
        }];

        let mut finding = AuditRecord::candidate(
            "finding-1",
            "Cross-crate issue",
            VerificationStatus::unverified("pending"),
        );
        finding.kind = AuditRecordKind::Finding;
        finding.severity = Some(Severity::High);
        finding
            .locations
            .push(audit_agent_core::finding::CodeLocation {
                crate_name: "other-crate".to_string(),
                module: "other_crate::lib".to_string(),
                file: PathBuf::from("/tmp/repo/other-crate/src/lib.rs"),
                line_range: (4, 4),
                snippet: None,
            });

        annotate_graph_with_findings(&mut nodes, &[finding], Some(Path::new("/tmp/repo")));

        assert_eq!(nodes[0].finding_count, None);
        assert_eq!(nodes[0].max_severity, None);
    }

    #[test]
    fn console_entry_marks_provider_failover_as_warning() {
        let event = session_store::SessionEvent {
            event_id: "evt-failover".to_string(),
            event_type: "provider.failover".to_string(),
            payload: serde_json::to_string(&orchestrator::AuditEvent::ProviderFailover {
                from: "openai".to_string(),
                to: "template-fallback".to_string(),
                role: "Scaffolding".to_string(),
                reason: "transient error".to_string(),
            })
            .expect("serialize failover event"),
            created_at: Utc::now(),
        };

        let entry = console_entry_from_event(&event);
        assert_eq!(entry.level, SessionConsoleLevel::Warning);
        assert!(entry.message.contains("openai -> template-fallback"));
    }

    #[test]
    fn load_activity_summary_aggregates_llm_tool_review_and_engine_events() {
        let workspace = tempfile::tempdir().expect("tempdir");
        let mut state = UiSessionState::new(workspace.path().join(".audit-work"));
        let session = AuditSession {
            session_id: "sess-activity".to_string(),
            snapshot: project_snapshot_from_config(
                &test_audit_config(workspace.path()),
                "snap-activity".to_string(),
            ),
            selected_domains: vec![],
            ui_state: SessionUiState::default(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        state
            .session_store
            .as_ref()
            .expect("session store")
            .create_session(&session)
            .expect("create session");
        state
            .sessions
            .insert(session.session_id.clone(), session.clone());

        let store = state.session_store.as_ref().expect("session store");
        let now = Utc::now();
        let events = vec![
            session_store::SessionEvent {
                event_id: "evt-llm".to_string(),
                event_type: "llm.interaction".to_string(),
                payload: serde_json::json!({
                    "provider": "openai",
                    "model": "gpt-4.1-mini",
                    "role": "SearchHints",
                    "duration_ms": 120,
                    "prompt_chars": 300,
                    "response_chars": 600,
                    "attempt": 1,
                    "succeeded": true
                })
                .to_string(),
                created_at: now,
            },
            session_store::SessionEvent {
                event_id: "evt-tool".to_string(),
                event_type: "tool.action.completed".to_string(),
                payload: serde_json::json!({
                    "action_id": "action-1",
                    "tool_family": "kani",
                    "target": "crate-a",
                    "status": "Completed",
                    "duration_ms": 8
                })
                .to_string(),
                created_at: now + chrono::TimeDelta::seconds(1),
            },
            session_store::SessionEvent {
                event_id: "evt-review".to_string(),
                event_type: "review.decision".to_string(),
                payload: serde_json::json!({
                    "record_id": "cand-1",
                    "action": "confirm",
                    "analyst_note": "validated"
                })
                .to_string(),
                created_at: now + chrono::TimeDelta::seconds(2),
            },
            session_store::SessionEvent {
                event_id: "evt-engine-ok".to_string(),
                event_type: "engine.completed".to_string(),
                payload: serde_json::json!({
                    "engine": "crypto_zk",
                    "findings_count": 2,
                    "duration_ms": 33
                })
                .to_string(),
                created_at: now + chrono::TimeDelta::seconds(3),
            },
            session_store::SessionEvent {
                event_id: "evt-engine-fail".to_string(),
                event_type: "engine.failed".to_string(),
                payload: serde_json::json!({
                    "engine": "distributed",
                    "reason": "timeout"
                })
                .to_string(),
                created_at: now + chrono::TimeDelta::seconds(4),
            },
        ];

        for event in &events {
            store
                .append_event(&session.session_id, event)
                .expect("append event");
        }

        let summary = state
            .load_activity_summary(&session.session_id)
            .expect("load activity summary");
        assert_eq!(summary.session_id, session.session_id);
        assert_eq!(summary.total_events, 5);
        assert_eq!(summary.total_duration_ms, 161);

        assert_eq!(summary.llm_calls.len(), 1);
        let llm = &summary.llm_calls[0];
        assert_eq!(llm.role, "SearchHints");
        assert_eq!(llm.count, 1);
        assert_eq!(llm.avg_duration_ms, 120);
        assert_eq!(llm.total_prompt_chars, 300);
        assert_eq!(llm.total_response_chars, 600);
        assert_eq!(llm.providers_used, vec!["openai".to_string()]);
        assert_eq!(llm.succeeded, 1);
        assert_eq!(llm.failed, 0);

        assert_eq!(summary.tool_actions.len(), 1);
        let tool = &summary.tool_actions[0];
        assert_eq!(tool.tool_family, "kani");
        assert_eq!(tool.count, 1);
        assert_eq!(tool.succeeded, 1);
        assert_eq!(tool.failed, 0);
        assert_eq!(tool.avg_duration_ms, 8);

        assert_eq!(summary.review_decisions.len(), 1);
        assert_eq!(summary.review_decisions[0].action, "confirm");
        assert_eq!(summary.review_decisions[0].count, 1);

        assert_eq!(summary.engine_outcomes.len(), 2);
        assert!(
            summary
                .engine_outcomes
                .iter()
                .any(|item| item.engine == "crypto_zk"
                    && item.status == "completed"
                    && item.findings_count == 2
                    && item.duration_ms == 33)
        );
        assert!(
            summary
                .engine_outcomes
                .iter()
                .any(|item| item.engine == "distributed"
                    && item.status == "failed"
                    && item.findings_count == 0
                    && item.duration_ms == 0)
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn load_audit_plan_returns_generated_plan_after_toolbench_context() {
        let workspace = tempfile::tempdir().expect("tempdir");
        fs::write(
            workspace.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"demo\"]\nresolver = \"2\"\n",
        )
        .expect("write workspace manifest");
        fs::create_dir_all(workspace.path().join("demo/src")).expect("create src");
        fs::write(
            workspace.path().join("demo/Cargo.toml"),
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .expect("write member manifest");
        fs::write(
            workspace.path().join("demo/src/lib.rs"),
            "pub fn demo() {}\n",
        )
        .expect("write source");

        let mut state = UiSessionState::new(workspace.path().join(".audit-work"));
        let mut config = test_audit_config(workspace.path());
        config.output_dir = workspace.path().join("audit-output");
        config.scope.target_crates = vec!["demo".to_string()];
        state.audit_config = Some(config.clone());

        let session = AuditSession {
            session_id: "sess-plan".to_string(),
            snapshot: project_snapshot_from_config(&config, "snap-plan".to_string()),
            selected_domains: vec!["crypto".to_string()],
            ui_state: SessionUiState::default(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        state
            .session_store
            .as_ref()
            .expect("session store")
            .create_session(&session)
            .expect("create session");
        state
            .sessions
            .insert(session.session_id.clone(), session.clone());

        state
            .load_toolbench_context(
                &session.session_id,
                ToolbenchSelectionRequest {
                    kind: "session".to_string(),
                    id: session.session_id.clone(),
                },
            )
            .await
            .expect("load toolbench context");

        let plan = state
            .load_audit_plan(&session.session_id)
            .expect("load audit plan");
        assert_eq!(plan.session_id, session.session_id);
        assert!(!plan.plan_id.is_empty());
        assert!(
            !plan.recommended_tools.is_empty(),
            "generated plan should include tool recommendations"
        );

        state
            .load_toolbench_context(
                &session.session_id,
                ToolbenchSelectionRequest {
                    kind: "session".to_string(),
                    id: session.session_id.clone(),
                },
            )
            .await
            .expect("load toolbench context twice");

        let events = state
            .session_store
            .as_ref()
            .expect("session store")
            .list_events_by_type(&session.session_id, "audit.plan.generated")
            .expect("list plan events");
        assert_eq!(
            events.len(),
            1,
            "load_toolbench_context should not append duplicate audit plans"
        );

        let reloaded = state
            .load_audit_plan(&session.session_id)
            .expect("reload audit plan");
        assert_eq!(
            reloaded.plan_id, plan.plan_id,
            "plan id should remain stable after repeated context loads"
        );
    }
}
