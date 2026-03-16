use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use audit_agent_core::audit_config::{AuditConfig, BudgetConfig, ResolvedSource, SourceOrigin};
use audit_agent_core::finding::{Severity, VerificationStatus};
use audit_agent_core::output::AuditManifest;
use audit_agent_core::session::{AuditRecord, AuditRecordKind, AuditSession, SessionUiState};
use chrono::Utc;
use intake::config::{ConfigParser, RawEngineConfig, RawScope, RawSource, ValidatedConfig};
use intake::confirmation::{ConfirmationSummary, UserDecisions};
use intake::project_snapshot_from_config;
use intake::source::SourceInput;
use knowledge::KnowledgeBase;
use knowledge::models::AdjudicatedCase;
use orchestrator::{AuditJob, AuditJobKind, AuditOrchestrator};
use project_ir::{
    ChecklistPlan as IrChecklistPlan, ProjectIr, ProjectIrBuilder,
    SecurityOverview as IrSecurityOverview,
};
use serde::{Deserialize, Serialize};
use session_store::SessionStore;

use crate::{
    ConfigParseResponse, OutputType, ResolvedSourceView, confirm_workspace, detect_workspace,
    download_output, export_audit_yaml, get_audit_manifest, resolve_source,
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
pub struct LoadToolbenchContextResponse {
    pub session_id: String,
    pub selection: ToolbenchSelectionResponse,
    pub recommended_tools: Vec<ToolbenchRecommendationResponse>,
    pub domains: Vec<ChecklistDomainPlanResponse>,
    pub overview_notes: Vec<String>,
    pub similar_cases: Vec<ToolbenchSimilarCaseResponse>,
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
    session_store: Option<Arc<SessionStore>>,
}

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
            session_store,
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

        let mut remaining = MAX_PROJECT_TREE_NODES;
        let nodes = collect_tree_nodes(&root_dir, &root_dir, 0, &mut remaining)?;

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

    pub async fn load_file_graph(&mut self, session_id: &str) -> Result<ProjectGraphResponse> {
        let ir = self.load_or_build_project_ir(session_id, false).await?;
        Ok(file_graph_response(session_id, &ir))
    }

    pub async fn load_feature_graph(&mut self, session_id: &str) -> Result<ProjectGraphResponse> {
        let ir = self.load_or_build_project_ir(session_id, false).await?;
        Ok(feature_graph_response(session_id, &ir))
    }

    pub async fn load_dataflow_graph(
        &mut self,
        session_id: &str,
        include_values: bool,
    ) -> Result<ProjectGraphResponse> {
        let ir = self
            .load_or_build_project_ir(session_id, include_values)
            .await?;
        Ok(dataflow_graph_response(session_id, &ir, !include_values))
    }

    pub async fn load_security_overview(
        &mut self,
        session_id: &str,
    ) -> Result<LoadSecurityOverviewResponse> {
        let ir = self.load_or_build_project_ir(session_id, false).await?;
        Ok(security_overview_response(
            session_id,
            ir.security_overview(),
        ))
    }

    pub async fn load_checklist_plan(
        &mut self,
        session_id: &str,
    ) -> Result<LoadChecklistPlanResponse> {
        let ir = self.load_or_build_project_ir(session_id, false).await?;
        Ok(checklist_plan_response(session_id, ir.checklist_plan()))
    }

    pub async fn load_toolbench_context(
        &mut self,
        session_id: &str,
        selection: ToolbenchSelectionRequest,
    ) -> Result<LoadToolbenchContextResponse> {
        let ir = self.load_or_build_project_ir(session_id, false).await?;
        let overview = ir.security_overview();
        let checklist = ir.checklist_plan();
        let context_terms = toolbench_context_terms(&selection, &checklist, &overview);

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
        })
    }

    pub async fn load_review_queue(&mut self, session_id: &str) -> Result<LoadReviewQueueResponse> {
        let _session = self.ensure_session_loaded(session_id)?;
        let mut records = self.load_candidate_records(session_id)?;

        if records.is_empty() {
            let seeded = self.seed_default_candidate(session_id).await?;
            records.push(seeded);
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
        self.persist_review_record(session_id, &record)?;
        Ok(record)
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

        bail!("unknown audit session `{session_id}`")
    }

    fn knowledge_feedback_store_path(&self) -> PathBuf {
        self.work_dir.join("knowledge-feedback.yaml")
    }

    fn load_knowledge_base(&self) -> Result<KnowledgeBase> {
        KnowledgeBase::load_from_repo_root_with_store(self.knowledge_feedback_store_path())
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
        let children = collect_tree_nodes(root_dir, &path, depth + 1, remaining)?;

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

    terms
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
        output_dir: PathBuf::from("audit-output"),
    }
}

#[cfg(test)]
mod tests {
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
}
