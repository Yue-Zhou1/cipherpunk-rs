use std::path::PathBuf;

use audit_agent_core::output::AuditManifest;
use intake::confirmation::CrateDecision;
use serde::Serialize;
use tauri::State;
use tauri_ui::ConfigParseResponse;
use tauri_ui::OutputType;
use tauri_ui::ipc::{
    ActivitySummary, ApplyReviewDecisionRequest, ApplyReviewDecisionResponse, AuditPlanResponse,
    AuditSessionSummary, ConfirmWorkspaceRequest, ConfirmWorkspaceResponse,
    CreateAuditSessionResponse, DownloadOutputResponse, ExplorerDepth, ExplorerGraphResponse,
    GetProjectTreeResponse, LoadChecklistPlanResponse, LoadReviewQueueResponse,
    LoadSecurityOverviewResponse, LoadToolbenchContextResponse, OpenAuditSessionResponse,
    ProjectGraphResponse, ReadSourceFileResponse, SourceInputIpc, TailSessionConsoleResponse,
    ToolbenchSelectionRequest,
};
use tauri_ui::{branch_resolution_banner, warning_message};

use crate::AppState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveSourceResponse {
    pub commit_hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch_resolution_banner: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParseConfigStatusResponse {
    pub status: &'static str,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectWorkspaceResponse {
    pub crate_count: usize,
    pub crates: Vec<CrateSummaryResponse>,
    pub frameworks: Vec<String>,
    pub warnings: Vec<String>,
    pub build_matrix: Vec<BuildVariantSummaryResponse>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CrateSummaryResponse {
    pub name: String,
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildVariantSummaryResponse {
    pub variant: String,
    pub features: String,
    pub est_time: String,
}

#[tauri::command]
pub async fn resolve_source(
    state: State<'_, AppState>,
    input: SourceInputIpc,
) -> Result<ResolveSourceResponse, String> {
    let mut session = state.session.lock().await;
    let resolved = session
        .resolve_source(input)
        .await
        .map_err(|err| err.to_string())?;
    Ok(ResolveSourceResponse {
        commit_hash: resolved.source.commit_hash,
        branch_resolution_banner: branch_resolution_banner(&resolved.warnings),
        warnings: resolved.warnings.iter().map(warning_message).collect(),
    })
}

#[tauri::command]
pub async fn parse_config(
    state: State<'_, AppState>,
    path: String,
) -> Result<ParseConfigStatusResponse, String> {
    let mut session = state.session.lock().await;
    let response = session.parse_config(PathBuf::from(path).as_path());

    Ok(match response {
        ConfigParseResponse::Validated { .. } => ParseConfigStatusResponse {
            status: "validated",
            errors: vec![],
        },
        ConfigParseResponse::ConfigErrors { errors } => ParseConfigStatusResponse {
            status: "errors",
            errors,
        },
    })
}

#[tauri::command]
pub async fn detect_workspace(
    state: State<'_, AppState>,
) -> Result<DetectWorkspaceResponse, String> {
    let mut session = state.session.lock().await;
    let summary = session.detect_workspace().map_err(|err| err.to_string())?;
    let source_warnings = session
        .resolved_source()
        .map(|resolved| {
            resolved
                .warnings
                .iter()
                .map(warning_message)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut warnings = source_warnings;
    warnings.extend(summary.warnings.iter().map(warning_message));

    let crate_rows = summary
        .crates
        .iter()
        .map(|decision| match decision {
            CrateDecision::InScope { meta } => CrateSummaryResponse {
                name: meta.name.clone(),
                status: "in_scope",
                reason: None,
            },
            CrateDecision::Excluded { meta, reason } => CrateSummaryResponse {
                name: meta.name.clone(),
                status: "excluded",
                reason: Some(reason.clone()),
            },
            CrateDecision::Ambiguous { meta, suggestion } => CrateSummaryResponse {
                name: meta.name.clone(),
                status: "ambiguous",
                reason: Some(suggestion.clone()),
            },
        })
        .collect::<Vec<_>>();

    let frameworks = summary
        .frameworks
        .iter()
        .map(|detected| format!("{:?}", detected.framework))
        .collect::<Vec<_>>();

    let per_variant_mins = if summary.build_matrix.is_empty() {
        summary.estimated_duration_mins
    } else {
        summary
            .estimated_duration_mins
            .div_ceil(summary.build_matrix.len() as u64)
    };
    let build_matrix = summary
        .build_matrix
        .iter()
        .map(|variant| BuildVariantSummaryResponse {
            variant: variant.label.clone(),
            features: if variant.features.is_empty() {
                "default".to_string()
            } else {
                variant.features.join(" + ")
            },
            est_time: format!("~{} min", per_variant_mins.max(1)),
        })
        .collect::<Vec<_>>();

    Ok(DetectWorkspaceResponse {
        crate_count: summary.crates.len(),
        crates: crate_rows,
        frameworks,
        warnings,
        build_matrix,
    })
}

#[tauri::command]
pub async fn confirm_workspace(
    state: State<'_, AppState>,
    decisions: ConfirmWorkspaceRequest,
) -> Result<ConfirmWorkspaceResponse, String> {
    let mut session = state.session.lock().await;
    session
        .confirm_workspace(decisions)
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn create_audit_session(
    state: State<'_, AppState>,
) -> Result<CreateAuditSessionResponse, String> {
    let mut session = state.session.lock().await;
    session
        .create_audit_session()
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn list_audit_sessions(
    state: State<'_, AppState>,
) -> Result<Vec<AuditSessionSummary>, String> {
    let session = state.session.lock().await;
    session.list_audit_sessions().map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn open_audit_session(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<Option<OpenAuditSessionResponse>, String> {
    let mut session = state.session.lock().await;
    session
        .open_audit_session(&session_id)
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn get_project_tree(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<GetProjectTreeResponse, String> {
    let mut session = state.session.lock().await;
    session
        .get_project_tree(&session_id)
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn read_source_file(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> Result<ReadSourceFileResponse, String> {
    let mut session = state.session.lock().await;
    session
        .read_source_file(&session_id, &path)
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn tail_session_console(
    state: State<'_, AppState>,
    session_id: String,
    limit: Option<usize>,
) -> Result<TailSessionConsoleResponse, String> {
    let mut session = state.session.lock().await;
    session
        .tail_session_console(&session_id, limit.unwrap_or(80))
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn load_activity_summary(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<ActivitySummary, String> {
    let session = state.session.lock().await;
    session
        .load_activity_summary(&session_id)
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn load_audit_plan(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<AuditPlanResponse, String> {
    let session = state.session.lock().await;
    session
        .load_audit_plan(&session_id)
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn load_explorer_graph(
    state: State<'_, AppState>,
    session_id: String,
    depth: Option<String>,
    cluster: Option<String>,
) -> Result<ExplorerGraphResponse, String> {
    let resolved_depth = match depth.as_deref() {
        Some("full") => ExplorerDepth::Full,
        _ => ExplorerDepth::Overview,
    };

    if resolved_depth == ExplorerDepth::Full && cluster.is_some() {
        return Err("Cannot specify both depth=full and cluster parameter".to_string());
    }

    let mut session = state.session.lock().await;
    session
        .load_explorer_graph(&session_id, resolved_depth, cluster.as_deref())
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn load_file_graph(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<ProjectGraphResponse, String> {
    let mut session = state.session.lock().await;
    session
        .load_file_graph(&session_id)
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn load_feature_graph(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<ProjectGraphResponse, String> {
    let mut session = state.session.lock().await;
    session
        .load_feature_graph(&session_id)
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn load_dataflow_graph(
    state: State<'_, AppState>,
    session_id: String,
    include_values: Option<bool>,
) -> Result<ProjectGraphResponse, String> {
    let mut session = state.session.lock().await;
    session
        .load_dataflow_graph(&session_id, include_values.unwrap_or(false))
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn load_symbol_graph(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<ProjectGraphResponse, String> {
    let mut session = state.session.lock().await;
    session
        .load_symbol_graph(&session_id)
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn load_security_overview(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<LoadSecurityOverviewResponse, String> {
    let mut session = state.session.lock().await;
    session
        .load_security_overview(&session_id)
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn load_checklist_plan(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<LoadChecklistPlanResponse, String> {
    let mut session = state.session.lock().await;
    session
        .load_checklist_plan(&session_id)
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn load_toolbench_context(
    state: State<'_, AppState>,
    session_id: String,
    selection: ToolbenchSelectionRequest,
) -> Result<LoadToolbenchContextResponse, String> {
    let mut session = state.session.lock().await;
    session
        .load_toolbench_context(&session_id, selection)
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn load_review_queue(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<LoadReviewQueueResponse, String> {
    let mut session = state.session.lock().await;
    session
        .load_review_queue(&session_id)
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn apply_review_decision(
    state: State<'_, AppState>,
    session_id: String,
    request: ApplyReviewDecisionRequest,
) -> Result<ApplyReviewDecisionResponse, String> {
    let mut session = state.session.lock().await;
    session
        .apply_review_decision(&session_id, request)
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn export_audit_yaml(state: State<'_, AppState>, path: String) -> Result<(), String> {
    let session = state.session.lock().await;
    session
        .export_audit_yaml(PathBuf::from(path).as_path())
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn get_audit_manifest(state: State<'_, AppState>) -> Result<AuditManifest, String> {
    let session = state.session.lock().await;
    session.get_audit_manifest().map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn download_output(
    state: State<'_, AppState>,
    audit_id: String,
    output_type: OutputType,
    dest: String,
) -> Result<DownloadOutputResponse, String> {
    let session = state.session.lock().await;
    let dest_path = PathBuf::from(dest);
    session
        .download_output(&audit_id, output_type, dest_path.as_path())
        .map_err(|err| err.to_string())
}
