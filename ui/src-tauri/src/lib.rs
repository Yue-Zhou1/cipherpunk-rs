mod commands;

use std::path::PathBuf;

use tauri_ui::ipc::UiSessionState;
use tokio::sync::Mutex;

pub struct AppState {
    pub session: Mutex<UiSessionState>,
}

impl AppState {
    fn new(work_dir: PathBuf) -> Self {
        Self {
            session: Mutex::new(UiSessionState::new(work_dir)),
        }
    }
}

fn default_work_dir() -> PathBuf {
    if let Ok(path) = std::env::var("AUDIT_AGENT_WORK_DIR") {
        return PathBuf::from(path);
    }

    std::env::current_dir()
        .map(|dir| dir.join(".audit-work"))
        .unwrap_or_else(|_| PathBuf::from(".audit-work"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::new(default_work_dir()))
        .invoke_handler(tauri::generate_handler![
            commands::resolve_source,
            commands::parse_config,
            commands::detect_workspace,
            commands::confirm_workspace,
            commands::create_audit_session,
            commands::list_audit_sessions,
            commands::open_audit_session,
            commands::get_project_tree,
            commands::read_source_file,
            commands::tail_session_console,
            commands::load_activity_summary,
            commands::load_audit_plan,
            commands::load_file_graph,
            commands::load_feature_graph,
            commands::load_dataflow_graph,
            commands::load_symbol_graph,
            commands::load_security_overview,
            commands::load_checklist_plan,
            commands::load_toolbench_context,
            commands::load_review_queue,
            commands::apply_review_decision,
            commands::export_audit_yaml,
            commands::get_audit_manifest,
            commands::download_output
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
