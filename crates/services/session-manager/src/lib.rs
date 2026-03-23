mod state;
mod workflow;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Error as AnyhowError;
use thiserror::Error;
use tokio::sync::{Mutex, RwLock};

pub use intake::confirmation::CrateDecision;
pub use state::*;
pub use workflow::*;

pub type SessionResult<T> = std::result::Result<T, SessionManagerError>;

#[derive(Debug, Error)]
#[error("unknown audit session `{session_id}`")]
pub(crate) struct UnknownSessionError {
    pub(crate) session_id: String,
}

#[derive(Debug, Error)]
pub enum SessionManagerError {
    #[error("{message}")]
    BadRequest { message: String },
    #[error("{message}")]
    NotFound { message: String },
    #[error("No session with id '{session_id}'")]
    SessionNotFound { session_id: String },
    #[error("{message}")]
    Internal { message: String },
}

impl SessionManagerError {
    pub fn code(&self) -> &'static str {
        match self {
            SessionManagerError::BadRequest { .. } => "BAD_REQUEST",
            SessionManagerError::NotFound { .. } => "NOT_FOUND",
            SessionManagerError::SessionNotFound { .. } => "SESSION_NOT_FOUND",
            SessionManagerError::Internal { .. } => "INTERNAL_ERROR",
        }
    }
}

fn map_state_error(err: AnyhowError) -> SessionManagerError {
    let message = err.to_string();
    for cause in err.chain() {
        if let Some(session_error) = cause.downcast_ref::<UnknownSessionError>() {
            return SessionManagerError::SessionNotFound {
                session_id: session_error.session_id.clone(),
            };
        }
        if let Some(io_error) = cause.downcast_ref::<std::io::Error>() {
            if io_error.kind() == std::io::ErrorKind::NotFound {
                return SessionManagerError::NotFound { message };
            }
            if matches!(
                io_error.kind(),
                std::io::ErrorKind::InvalidInput | std::io::ErrorKind::PermissionDenied
            ) {
                return SessionManagerError::BadRequest { message };
            }
        }
    }

    if message.contains("unknown audit session") || message.contains("No session with id") {
        return SessionManagerError::NotFound { message };
    }
    if message.contains("ProjectIR has not been built for this session") {
        return SessionManagerError::NotFound { message };
    }
    if message.contains("not found") || message.contains("path does not exist") {
        return SessionManagerError::NotFound { message };
    }
    if message.contains("must be called before")
        || message.contains("requires")
        || message.contains("invalid")
        || message.contains("declined by user")
        || message.contains("outside of the session source root")
        || message.contains("path traversal")
    {
        return SessionManagerError::BadRequest { message };
    }
    SessionManagerError::Internal { message }
}

pub struct SessionManager {
    work_dir: PathBuf,
    inner: Arc<Mutex<UiSessionState>>,
    wizard_states: RwLock<HashMap<String, WizardStateEntry>>,
    wizard_ttl: Duration,
}

#[derive(Clone)]
struct WizardStateEntry {
    state: Arc<Mutex<UiSessionState>>,
    touched_at: Instant,
}

impl SessionManager {
    const DEFAULT_WIZARD_TTL: Duration = Duration::from_secs(30 * 60);

    pub fn new(work_dir: PathBuf) -> Self {
        let initial_state = Arc::new(Mutex::new(UiSessionState::new(work_dir.clone())));
        Self {
            work_dir,
            inner: initial_state,
            wizard_states: RwLock::new(HashMap::new()),
            wizard_ttl: Self::DEFAULT_WIZARD_TTL,
        }
    }

    pub async fn resolve_source(&self, input: SourceInputIpc) -> SessionResult<ResolvedSourceView> {
        self.resolve_source_with_wizard(None, input).await
    }

    pub async fn resolve_source_with_wizard(
        &self,
        wizard_id: Option<&str>,
        input: SourceInputIpc,
    ) -> SessionResult<ResolvedSourceView> {
        let state = self.state_for_wizard(wizard_id).await;
        let mut state = state.lock().await;
        state.resolve_source(input).await.map_err(map_state_error)
    }

    pub async fn parse_config(&self, path: PathBuf) -> ConfigParseResponse {
        self.parse_config_with_wizard(None, path).await
    }

    pub async fn parse_config_with_wizard(
        &self,
        wizard_id: Option<&str>,
        path: PathBuf,
    ) -> ConfigParseResponse {
        let state = self.state_for_wizard(wizard_id).await;
        let mut state = state.lock().await;
        state.parse_config(&path)
    }

    pub async fn detect_workspace(
        &self,
    ) -> SessionResult<intake::confirmation::ConfirmationSummary> {
        self.detect_workspace_with_wizard(None).await
    }

    pub async fn detect_workspace_with_wizard(
        &self,
        wizard_id: Option<&str>,
    ) -> SessionResult<intake::confirmation::ConfirmationSummary> {
        let state = self.state_for_wizard(wizard_id).await;
        let mut state = state.lock().await;
        state.detect_workspace().map_err(map_state_error)
    }

    pub async fn confirm_workspace(
        &self,
        request: ConfirmWorkspaceRequest,
    ) -> SessionResult<ConfirmWorkspaceResponse> {
        self.confirm_workspace_with_wizard(None, request).await
    }

    pub async fn confirm_workspace_with_wizard(
        &self,
        wizard_id: Option<&str>,
        request: ConfirmWorkspaceRequest,
    ) -> SessionResult<ConfirmWorkspaceResponse> {
        let state = self.state_for_wizard(wizard_id).await;
        let mut state = state.lock().await;
        state.confirm_workspace(request).map_err(map_state_error)
    }

    pub async fn create_audit_session(&self) -> SessionResult<CreateAuditSessionResponse> {
        self.create_audit_session_with_wizard(None).await
    }

    pub async fn create_audit_session_with_wizard(
        &self,
        wizard_id: Option<&str>,
    ) -> SessionResult<CreateAuditSessionResponse> {
        let state = self.state_for_wizard(wizard_id).await;
        let mut state = state.lock().await;
        state.create_audit_session().await.map_err(map_state_error)
    }

    pub async fn list_audit_sessions(&self) -> SessionResult<Vec<AuditSessionSummary>> {
        let state = self.inner.lock().await;
        state.list_audit_sessions().map_err(map_state_error)
    }

    pub async fn open_audit_session(
        &self,
        session_id: &str,
    ) -> SessionResult<Option<OpenAuditSessionResponse>> {
        let mut state = self.inner.lock().await;
        state
            .open_audit_session(session_id)
            .await
            .map_err(map_state_error)
    }

    pub async fn get_project_tree(
        &self,
        session_id: &str,
    ) -> SessionResult<GetProjectTreeResponse> {
        let mut state = self.inner.lock().await;
        state
            .get_project_tree(session_id)
            .await
            .map_err(map_state_error)
    }

    pub async fn read_source_file(
        &self,
        session_id: &str,
        path: &str,
    ) -> SessionResult<ReadSourceFileResponse> {
        let mut state = self.inner.lock().await;
        state
            .read_source_file(session_id, path)
            .await
            .map_err(map_state_error)
    }

    pub async fn load_file_graph(&self, session_id: &str) -> SessionResult<ProjectGraphResponse> {
        let mut state = self.inner.lock().await;
        state
            .load_file_graph(session_id)
            .await
            .map_err(map_state_error)
    }

    pub async fn load_feature_graph(
        &self,
        session_id: &str,
    ) -> SessionResult<ProjectGraphResponse> {
        let mut state = self.inner.lock().await;
        state
            .load_feature_graph(session_id)
            .await
            .map_err(map_state_error)
    }

    pub async fn load_dataflow_graph(
        &self,
        session_id: &str,
        include_values: bool,
    ) -> SessionResult<ProjectGraphResponse> {
        let mut state = self.inner.lock().await;
        state
            .load_dataflow_graph(session_id, include_values)
            .await
            .map_err(map_state_error)
    }

    pub async fn load_symbol_graph(&self, session_id: &str) -> SessionResult<ProjectGraphResponse> {
        let mut state = self.inner.lock().await;
        state
            .load_symbol_graph(session_id)
            .await
            .map_err(map_state_error)
    }

    pub async fn load_security_overview(
        &self,
        session_id: &str,
    ) -> SessionResult<LoadSecurityOverviewResponse> {
        let mut state = self.inner.lock().await;
        state
            .load_security_overview(session_id)
            .await
            .map_err(map_state_error)
    }

    pub async fn load_checklist_plan(
        &self,
        session_id: &str,
    ) -> SessionResult<LoadChecklistPlanResponse> {
        let mut state = self.inner.lock().await;
        state
            .load_checklist_plan(session_id)
            .await
            .map_err(map_state_error)
    }

    pub async fn load_toolbench_context(
        &self,
        session_id: &str,
        selection: ToolbenchSelectionRequest,
    ) -> SessionResult<LoadToolbenchContextResponse> {
        let mut state = self.inner.lock().await;
        state
            .load_toolbench_context(session_id, selection)
            .await
            .map_err(map_state_error)
    }

    pub async fn load_review_queue(
        &self,
        session_id: &str,
    ) -> SessionResult<LoadReviewQueueResponse> {
        let mut state = self.inner.lock().await;
        state
            .load_review_queue(session_id)
            .await
            .map_err(map_state_error)
    }

    pub async fn apply_review_decision(
        &self,
        session_id: &str,
        request: ApplyReviewDecisionRequest,
    ) -> SessionResult<ApplyReviewDecisionResponse> {
        let mut state = self.inner.lock().await;
        state
            .apply_review_decision(session_id, request)
            .await
            .map_err(map_state_error)
    }

    pub async fn tail_session_console(
        &self,
        session_id: &str,
        limit: usize,
    ) -> SessionResult<TailSessionConsoleResponse> {
        let mut state = self.inner.lock().await;
        state
            .tail_session_console(session_id, limit)
            .map_err(map_state_error)
    }

    pub async fn get_audit_manifest(
        &self,
    ) -> SessionResult<audit_agent_core::output::AuditManifest> {
        let state = self.inner.lock().await;
        state.get_audit_manifest().map_err(map_state_error)
    }

    pub async fn export_audit_yaml(&self, path: PathBuf) -> SessionResult<()> {
        let state = self.inner.lock().await;
        state.export_audit_yaml(&path).map_err(map_state_error)
    }

    pub async fn download_output(
        &self,
        audit_id: &str,
        output_type: OutputType,
        dest: PathBuf,
    ) -> SessionResult<PathBuf> {
        let state = self.inner.lock().await;
        state
            .download_output(audit_id, output_type, &dest)
            .map(|response| response.dest)
            .map_err(map_state_error)
    }

    async fn state_for_wizard(&self, wizard_id: Option<&str>) -> Arc<Mutex<UiSessionState>> {
        let Some(wizard_id) = normalize_wizard_id(wizard_id) else {
            return self.inner.clone();
        };

        let mut states = self.wizard_states.write().await;
        self.prune_expired_wizards_locked(&mut states);
        let entry = states.entry(wizard_id).or_insert_with(|| WizardStateEntry {
            state: Arc::new(Mutex::new(UiSessionState::new(self.work_dir.clone()))),
            touched_at: Instant::now(),
        });
        entry.touched_at = Instant::now();
        entry.state.clone()
    }

    fn prune_expired_wizards_locked(&self, states: &mut HashMap<String, WizardStateEntry>) {
        let now = Instant::now();
        states.retain(|_, entry| now.duration_since(entry.touched_at) <= self.wizard_ttl);
    }
}

fn normalize_wizard_id(wizard_id: Option<&str>) -> Option<String> {
    let trimmed = wizard_id?.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;

    use anyhow::anyhow;

    use super::{
        ConfirmWorkspaceRequest, SessionManager, SessionManagerError, SourceInputIpc, SourceKind,
        map_state_error,
    };

    #[tokio::test(flavor = "current_thread")]
    async fn confirm_workspace_requires_resolve_source_first() {
        let manager = SessionManager::new(PathBuf::from(".audit-work"));
        let err = manager
            .confirm_workspace(ConfirmWorkspaceRequest {
                confirmed: true,
                ambiguous_crates: HashMap::new(),
                no_llm_prose: false,
            })
            .await
            .expect_err("confirm_workspace should enforce wizard ordering");
        assert!(err.to_string().contains("resolve_source must be called"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn wizard_id_isolates_wizard_flow_state() {
        let workspace = tempfile::tempdir().expect("tempdir");
        fs::write(
            workspace.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"demo\"]\nresolver = \"2\"\n",
        )
        .expect("write root manifest");
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
        .expect("write member source");

        let git_init = Command::new("git")
            .arg("init")
            .arg("-q")
            .current_dir(workspace.path())
            .status()
            .expect("git must be available for resolve_source local mode");
        assert!(
            git_init.success(),
            "git init should succeed for test workspace"
        );
        let git_add = Command::new("git")
            .arg("add")
            .arg(".")
            .current_dir(workspace.path())
            .status()
            .expect("git add");
        assert!(git_add.success(), "git add should succeed");
        let git_commit = Command::new("git")
            .args([
                "-c",
                "user.name=Session Manager Test",
                "-c",
                "user.email=session-manager@test.invalid",
                "commit",
                "-qm",
                "initial",
            ])
            .current_dir(workspace.path())
            .status()
            .expect("git commit");
        assert!(git_commit.success(), "git commit should succeed");

        let manager = SessionManager::new(workspace.path().join(".audit-work"));
        manager
            .resolve_source_with_wizard(
                Some("wizard-a"),
                SourceInputIpc {
                    kind: SourceKind::Local,
                    value: workspace.path().display().to_string(),
                    commit_or_ref: None,
                },
            )
            .await
            .expect("resolve local source for wizard-a");
        manager
            .detect_workspace_with_wizard(Some("wizard-a"))
            .await
            .expect("wizard-a detect workspace");

        let err = manager
            .detect_workspace_with_wizard(Some("wizard-b"))
            .await
            .expect_err("wizard-b should not inherit wizard-a state");
        assert!(
            err.to_string().contains("resolve_source must be called"),
            "wizard state should be isolated per wizard_id"
        );
    }

    #[test]
    fn project_ir_not_built_message_maps_to_not_found() {
        let err = map_state_error(anyhow!(
            "ProjectIR has not been built for this session. Run BuildProjectIr first."
        ));
        assert!(matches!(err, SessionManagerError::NotFound { .. }));
    }
}
