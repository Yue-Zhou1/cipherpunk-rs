use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::header::CONTENT_TYPE;
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, get_service, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::json;
use session_manager::{
    ApplyReviewDecisionRequest, ApplyReviewDecisionResponse, AuditSessionSummary,
    ConfigParseResponse, ConfirmWorkspaceRequest, ConfirmWorkspaceResponse,
    CreateAuditSessionResponse, CrateDecision, GetProjectTreeResponse,
    LoadChecklistPlanResponse, LoadReviewQueueResponse, LoadSecurityOverviewResponse,
    LoadToolbenchContextResponse, OpenAuditSessionResponse, OutputType, ProjectGraphResponse,
    ReadSourceFileResponse, SessionConsoleEntry, SessionConsoleLevel, SessionManager,
    SessionManagerError, SourceInputIpc, TailSessionConsoleResponse, ToolbenchSelectionRequest,
    branch_resolution_banner, warning_message,
};
use tokio::time::{self, Duration};
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;

#[derive(Clone)]
pub struct AppState {
    pub manager: Arc<SessionManager>,
    pub events_poll_interval: Duration,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveSourceRequest {
    pub input: SourceInputIpc,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveSourceResponse {
    pub commit_hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch_resolution_banner: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParseConfigRequest {
    pub path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParseConfigResponse {
    pub status: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceCrateSummary {
    pub name: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildVariantSummary {
    pub variant: String,
    pub features: String,
    pub est_time: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectWorkspaceResponse {
    pub crate_count: usize,
    pub crates: Vec<WorkspaceCrateSummary>,
    pub frameworks: Vec<String>,
    pub warnings: Vec<String>,
    pub build_matrix: Vec<BuildVariantSummary>,
}

#[derive(Debug, Deserialize)]
pub struct GraphQuery {
    #[serde(default)]
    include_values: bool,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct WizardQuery {
    wizard_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmWorkspaceEnvelope {
    pub decisions: ConfirmWorkspaceRequest,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolbenchContextEnvelope {
    pub selection: ToolbenchSelectionRequest,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyReviewDecisionEnvelope {
    pub request: ApplyReviewDecisionRequest,
}

#[derive(Debug, Deserialize)]
pub struct TailConsoleQuery {
    #[serde(default = "default_console_limit")]
    limit: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportAuditYamlRequest {
    pub path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadOutputRequest {
    pub audit_id: String,
    pub output_type: OutputType,
    pub dest: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ApiErrorEnvelope {
    error: ApiErrorBody,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ApiErrorBody {
    code: String,
    message: String,
    status: u16,
}

#[derive(Debug)]
pub struct AppError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl AppError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "BAD_REQUEST",
            message: message.into(),
        }
    }

    fn not_found(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            code,
            message: message.into(),
        }
    }

    fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "INTERNAL_ERROR",
            message: message.into(),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let body = ApiErrorEnvelope {
            error: ApiErrorBody {
                code: self.code.to_string(),
                message: self.message,
                status: self.status.as_u16(),
            },
        };
        (self.status, Json(body)).into_response()
    }
}

fn map_session_error(err: SessionManagerError) -> AppError {
    match err {
        SessionManagerError::BadRequest { message } => AppError::bad_request(message),
        SessionManagerError::NotFound { message } => AppError::not_found("NOT_FOUND", message),
        SessionManagerError::SessionNotFound { session_id } => AppError::not_found(
            "SESSION_NOT_FOUND",
            format!("No session with id '{session_id}'"),
        ),
        SessionManagerError::Internal { message } => AppError::internal(message),
    }
}

pub fn build_app(
    state: AppState,
    static_dir: Option<PathBuf>,
    cors_origin: Option<String>,
) -> Router {
    let mut app = Router::new()
        .route("/api/source/resolve", post(resolve_source))
        .route("/api/config/parse", post(parse_config))
        .route("/api/workspace/detect", post(detect_workspace))
        .route("/api/workspace/confirm", post(confirm_workspace))
        .route(
            "/api/sessions",
            post(create_audit_session).get(list_audit_sessions),
        )
        .route("/api/sessions/:session_id", get(open_audit_session))
        .route("/api/sessions/:session_id/tree", get(get_project_tree))
        .route(
            "/api/sessions/:session_id/files/*path",
            get(read_source_file),
        )
        .route("/api/sessions/:session_id/graphs/:lens", get(load_graph))
        .route(
            "/api/sessions/:session_id/security",
            get(load_security_overview),
        )
        .route(
            "/api/sessions/:session_id/manifest",
            get(get_session_audit_manifest),
        )
        .route("/api/manifest", get(get_current_audit_manifest))
        .route("/api/sessions/:session_id/events", get(session_events_ws))
        .route(
            "/api/sessions/:session_id/console",
            get(tail_session_console),
        )
        .route(
            "/api/sessions/:session_id/checklist",
            get(load_checklist_plan),
        )
        .route(
            "/api/sessions/:session_id/toolbench",
            post(load_toolbench_context),
        )
        .route(
            "/api/sessions/:session_id/review-queue",
            get(load_review_queue),
        )
        .route(
            "/api/sessions/:session_id/review-decision",
            post(apply_review_decision),
        )
        .route("/api/export/audit-yaml", post(export_audit_yaml))
        .route("/api/output/download", post(download_output))
        .with_state(state);

    if let Some(cors) = build_cors_layer(cors_origin.as_deref()) {
        app = app.layer(cors);
    }

    if let Some(dir) = static_dir {
        let static_files = get_service(ServeDir::new(dir).append_index_html_on_directories(true));
        app = app.fallback_service(static_files);
    }

    app
}

fn build_cors_layer(cors_origin: Option<&str>) -> Option<CorsLayer> {
    let base = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([CONTENT_TYPE]);

    match cors_origin {
        Some(origin) => match origin.parse::<HeaderValue>() {
            Ok(value) => Some(base.allow_origin(value)),
            Err(_) => Some(base.allow_origin(Any)),
        },
        None => None,
    }
}

fn default_console_limit() -> usize {
    80
}

pub fn default_events_poll_interval() -> Duration {
    Duration::from_secs(2)
}

fn normalized_wizard_id(value: Option<&str>) -> Option<String> {
    let trimmed = value?.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn wizard_id_from_request(headers: &HeaderMap, query: &WizardQuery) -> Option<String> {
    normalized_wizard_id(query.wizard_id.as_deref()).or_else(|| {
        headers
            .get("x-wizard-id")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| normalized_wizard_id(Some(value)))
    })
}

async fn resolve_source(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<WizardQuery>,
    Json(input): Json<ResolveSourceRequest>,
) -> Result<Json<ResolveSourceResponse>, AppError> {
    let wizard_id = wizard_id_from_request(&headers, &query);
    let resolved = state
        .manager
        .resolve_source_with_wizard(wizard_id.as_deref(), input.input)
        .await
        .map_err(map_session_error)?;
    Ok(Json(ResolveSourceResponse {
        commit_hash: resolved.source.commit_hash,
        branch_resolution_banner: branch_resolution_banner(&resolved.warnings),
        warnings: resolved
            .warnings
            .iter()
            .map(warning_message)
            .collect::<Vec<_>>(),
    }))
}

async fn parse_config(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<WizardQuery>,
    Json(input): Json<ParseConfigRequest>,
) -> Result<Json<ParseConfigResponse>, AppError> {
    let wizard_id = wizard_id_from_request(&headers, &query);
    let response = state
        .manager
        .parse_config_with_wizard(wizard_id.as_deref(), PathBuf::from(input.path))
        .await;
    let status = match response {
        ConfigParseResponse::Validated { .. } => "validated",
        ConfigParseResponse::ConfigErrors { .. } => "errors",
    };
    Ok(Json(ParseConfigResponse {
        status: status.to_string(),
    }))
}

async fn detect_workspace(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<WizardQuery>,
) -> Result<Json<DetectWorkspaceResponse>, AppError> {
    let wizard_id = wizard_id_from_request(&headers, &query);
    let summary = state
        .manager
        .detect_workspace_with_wizard(wizard_id.as_deref())
        .await
        .map_err(map_session_error)?;

    let crates = summary
        .crates
        .iter()
        .map(|decision| match decision {
            CrateDecision::InScope { meta } => WorkspaceCrateSummary {
                name: meta.name.clone(),
                status: "in_scope".to_string(),
                reason: None,
            },
            CrateDecision::Excluded { meta, reason } => WorkspaceCrateSummary {
                name: meta.name.clone(),
                status: "excluded".to_string(),
                reason: Some(reason.clone()),
            },
            CrateDecision::Ambiguous { meta, suggestion } => WorkspaceCrateSummary {
                name: meta.name.clone(),
                status: "ambiguous".to_string(),
                reason: Some(suggestion.clone()),
            },
        })
        .collect::<Vec<_>>();

    let est_time = format!("~{} min", summary.estimated_duration_mins.max(1));
    let build_matrix = summary
        .build_matrix
        .iter()
        .map(|variant| BuildVariantSummary {
            variant: variant.label.clone(),
            features: if variant.features.is_empty() {
                "default".to_string()
            } else {
                variant.features.join(" + ")
            },
            est_time: est_time.clone(),
        })
        .collect::<Vec<_>>();

    let warnings = summary
        .warnings
        .iter()
        .map(warning_message)
        .collect::<Vec<_>>();
    let frameworks = summary
        .frameworks
        .iter()
        .map(|entry| format!("{:?}", entry.framework))
        .collect::<Vec<_>>();

    Ok(Json(DetectWorkspaceResponse {
        crate_count: crates.len(),
        crates,
        frameworks,
        warnings,
        build_matrix,
    }))
}

async fn confirm_workspace(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<WizardQuery>,
    Json(input): Json<ConfirmWorkspaceEnvelope>,
) -> Result<Json<ConfirmWorkspaceResponse>, AppError> {
    let wizard_id = wizard_id_from_request(&headers, &query);
    let confirmed = state
        .manager
        .confirm_workspace_with_wizard(wizard_id.as_deref(), input.decisions)
        .await
        .map_err(map_session_error)?;
    Ok(Json(confirmed))
}

async fn create_audit_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<WizardQuery>,
) -> Result<Json<CreateAuditSessionResponse>, AppError> {
    let wizard_id = wizard_id_from_request(&headers, &query);
    let created = state
        .manager
        .create_audit_session_with_wizard(wizard_id.as_deref())
        .await
        .map_err(map_session_error)?;
    Ok(Json(created))
}

async fn list_audit_sessions(
    State(state): State<AppState>,
) -> Result<Json<Vec<AuditSessionSummary>>, AppError> {
    let sessions = state
        .manager
        .list_audit_sessions()
        .await
        .map_err(map_session_error)?;
    Ok(Json(sessions))
}

async fn open_audit_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<OpenAuditSessionResponse>, AppError> {
    let Some(session) = state
        .manager
        .open_audit_session(&session_id)
        .await
        .map_err(map_session_error)?
    else {
        return Err(AppError::not_found(
            "SESSION_NOT_FOUND",
            format!("No session with id '{session_id}'"),
        ));
    };
    Ok(Json(session))
}

async fn get_project_tree(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<GetProjectTreeResponse>, AppError> {
    let tree = state
        .manager
        .get_project_tree(&session_id)
        .await
        .map_err(map_session_error)?;
    Ok(Json(tree))
}

async fn read_source_file(
    State(state): State<AppState>,
    Path((session_id, path)): Path<(String, String)>,
) -> Result<Json<ReadSourceFileResponse>, AppError> {
    let file = state
        .manager
        .read_source_file(&session_id, &path)
        .await
        .map_err(map_session_error)?;
    Ok(Json(file))
}

async fn load_graph(
    State(state): State<AppState>,
    Path((session_id, lens)): Path<(String, String)>,
    Query(query): Query<GraphQuery>,
) -> Result<Json<ProjectGraphResponse>, AppError> {
    let graph = match lens.as_str() {
        "file" => state.manager.load_file_graph(&session_id).await,
        "feature" => state.manager.load_feature_graph(&session_id).await,
        "dataflow" => {
            state
                .manager
                .load_dataflow_graph(&session_id, query.include_values)
                .await
        }
        _ => {
            return Err(AppError::bad_request(format!(
                "unknown graph lens '{lens}'"
            )));
        }
    }
    .map_err(map_session_error)?;

    Ok(Json(graph))
}

async fn load_security_overview(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<LoadSecurityOverviewResponse>, AppError> {
    let response = state
        .manager
        .load_security_overview(&session_id)
        .await
        .map_err(map_session_error)?;
    Ok(Json(response))
}

async fn get_session_audit_manifest(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let _ = state
        .manager
        .open_audit_session(&session_id)
        .await
        .map_err(map_session_error)?;

    match state.manager.get_audit_manifest().await {
        Ok(manifest) => serde_json::to_value(manifest)
            .map(Json)
            .map_err(|err| AppError::internal(err.to_string())),
        Err(_) => Ok(Json(json!({
            "sessionId": session_id,
            "manifest": "unavailable"
        }))),
    }
}

async fn get_current_audit_manifest(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, AppError> {
    match state.manager.get_audit_manifest().await {
        Ok(manifest) => serde_json::to_value(manifest)
            .map(Json)
            .map_err(|err| AppError::internal(err.to_string())),
        Err(err) => Err(map_session_error(err)),
    }
}

async fn tail_session_console(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Query(query): Query<TailConsoleQuery>,
) -> Result<Json<TailSessionConsoleResponse>, AppError> {
    let response = state
        .manager
        .tail_session_console(&session_id, query.limit.max(1))
        .await
        .map_err(map_session_error)?;
    Ok(Json(response))
}

async fn load_checklist_plan(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<LoadChecklistPlanResponse>, AppError> {
    let response = state
        .manager
        .load_checklist_plan(&session_id)
        .await
        .map_err(map_session_error)?;
    Ok(Json(response))
}

async fn load_toolbench_context(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(input): Json<ToolbenchContextEnvelope>,
) -> Result<Json<LoadToolbenchContextResponse>, AppError> {
    let response = state
        .manager
        .load_toolbench_context(&session_id, input.selection)
        .await
        .map_err(map_session_error)?;
    Ok(Json(response))
}

async fn load_review_queue(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<LoadReviewQueueResponse>, AppError> {
    let response = state
        .manager
        .load_review_queue(&session_id)
        .await
        .map_err(map_session_error)?;
    Ok(Json(response))
}

async fn apply_review_decision(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(input): Json<ApplyReviewDecisionEnvelope>,
) -> Result<Json<ApplyReviewDecisionResponse>, AppError> {
    let response = state
        .manager
        .apply_review_decision(&session_id, input.request)
        .await
        .map_err(map_session_error)?;
    Ok(Json(response))
}

async fn export_audit_yaml(
    State(state): State<AppState>,
    Json(input): Json<ExportAuditYamlRequest>,
) -> Result<StatusCode, AppError> {
    state
        .manager
        .export_audit_yaml(PathBuf::from(input.path))
        .await
        .map_err(map_session_error)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn download_output(
    State(state): State<AppState>,
    Json(input): Json<DownloadOutputRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let dest = state
        .manager
        .download_output(
            &input.audit_id,
            input.output_type,
            PathBuf::from(input.dest),
        )
        .await
        .map_err(map_session_error)?;
    Ok(Json(json!({
        "dest": dest
    })))
}

async fn session_events_ws(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Response, AppError> {
    let Some(_session) = state
        .manager
        .open_audit_session(&session_id)
        .await
        .map_err(map_session_error)?
    else {
        return Err(AppError::not_found(
            "SESSION_NOT_FOUND",
            format!("No session with id '{session_id}'"),
        ));
    };

    let manager = state.manager.clone();
    let poll_interval = state.events_poll_interval;
    Ok(ws.on_upgrade(move |socket| async move {
        stream_session_events(socket, manager, session_id, poll_interval).await;
    }))
}

async fn stream_session_events(
    mut socket: WebSocket,
    manager: Arc<SessionManager>,
    session_id: String,
    poll_interval: Duration,
) {
    let mut ticker = time::interval(poll_interval);
    ticker.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
    let mut last_payload = String::new();

    loop {
        tokio::select! {
            maybe_message = socket.recv() => {
                match maybe_message {
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                    _ => {}
                }
            }
            _ = ticker.tick() => {
                let console = manager
                    .tail_session_console(&session_id, 30)
                    .await
                    .ok()
                    .map(|response| response.entries)
                    .unwrap_or_default();
                let payload = execution_payload(&session_id, &console).to_string();
                if payload == last_payload {
                    continue;
                }
                last_payload = payload.clone();
                if socket.send(Message::Text(payload)).await.is_err() {
                    break;
                }
            }
        }
    }
}

fn execution_payload(session_id: &str, entries: &[SessionConsoleEntry]) -> serde_json::Value {
    let logs = entries
        .iter()
        .map(|entry| {
            let channel = match entry.level {
                SessionConsoleLevel::Info => "intake",
                SessionConsoleLevel::Warning => "rules",
                SessionConsoleLevel::Error => "report",
            };
            json!({
                "timestamp": entry.timestamp,
                "channel": channel,
                "message": entry.message,
            })
        })
        .collect::<Vec<_>>();

    json!({
        "auditId": session_id,
        "nodes": [],
        "counts": {
            "critical": 0,
            "high": 0,
            "medium": 0,
            "low": 0,
            "observation": 0
        },
        "logs": logs,
        "latestFinding": ""
    })
}

pub fn default_work_dir() -> PathBuf {
    std::env::var("WORK_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(".audit-work"))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::body::{Body, to_bytes};
    use axum::http::{HeaderMap, Request, StatusCode};
    use tower::ServiceExt;

    use super::{AppState, SessionManager, WizardQuery, build_app, wizard_id_from_request};

    #[tokio::test(flavor = "current_thread")]
    async fn open_unknown_session_returns_error_envelope() {
        let app = build_app(
            AppState {
                manager: Arc::new(SessionManager::new(".audit-work".into())),
                events_poll_interval: super::default_events_poll_interval(),
            },
            None,
            None,
        );

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/sessions/sess-missing")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body = to_bytes(response.into_body(), 1024 * 32)
            .await
            .expect("body");
        let payload: serde_json::Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(payload["error"]["code"], "SESSION_NOT_FOUND");
        assert_eq!(payload["error"]["status"], 404);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn resolve_source_validation_error_uses_error_envelope() {
        let app = build_app(
            AppState {
                manager: Arc::new(SessionManager::new(".audit-work".into())),
                events_poll_interval: super::default_events_poll_interval(),
            },
            None,
            None,
        );

        let body = serde_json::json!({
            "input": {
                "kind": "git",
                "value": "https://github.com/example/repo"
            }
        })
        .to_string();
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/source/resolve")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let payload: serde_json::Value = serde_json::from_slice(
            &to_bytes(response.into_body(), 1024 * 32)
                .await
                .expect("body"),
        )
        .expect("json");
        assert_eq!(payload["error"]["status"], 400);
        assert_eq!(payload["error"]["code"], "BAD_REQUEST");
    }

    #[test]
    fn wizard_id_can_be_read_from_query_or_header() {
        let mut headers = HeaderMap::new();
        headers.insert("x-wizard-id", "wizard-header".parse().expect("header"));

        let query = WizardQuery {
            wizard_id: Some("wizard-query".to_string()),
        };
        assert_eq!(
            wizard_id_from_request(&headers, &query).as_deref(),
            Some("wizard-query")
        );

        let query_none = WizardQuery { wizard_id: None };
        assert_eq!(
            wizard_id_from_request(&headers, &query_none).as_deref(),
            Some("wizard-header")
        );
    }
}
