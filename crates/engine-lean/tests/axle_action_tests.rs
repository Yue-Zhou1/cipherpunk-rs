use audit_agent_core::tooling::{
    ToolActionRequest, ToolActionStatus, ToolBudget, ToolFamily, ToolTarget,
};
use engine_lean::tool_actions::axle::execute_lean_action;
use mockito::Server;
use std::io::Write;
use std::sync::{Mutex, OnceLock};
use tempfile::NamedTempFile;

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn lean_request(path: &str) -> ToolActionRequest {
    ToolActionRequest {
        session_id: "sess-test".to_string(),
        workspace_root: None,
        tool_family: ToolFamily::LeanExternal,
        target: ToolTarget::File {
            path: path.to_string(),
        },
        budget: ToolBudget::default(),
    }
}

async fn mock_check_ok(server: &mut mockito::ServerGuard) {
    server
        .mock("POST", "/check")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"okay":true,"content":"","lean_messages":{"errors":[],"warnings":[],"infos":[]},"tool_messages":{"errors":[],"warnings":[],"infos":[]},"failed_declarations":[]}"#)
        .create_async()
        .await;
}

async fn mock_sorry2lemma_with_lemmas(server: &mut mockito::ServerGuard) {
    server
        .mock("POST", "/sorry2lemma")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"content":"extracted content","lean_messages":{"errors":[],"warnings":[],"infos":[]},"lemma_names":["subgoal_0"]}"#)
        .create_async()
        .await;
}

async fn mock_disprove_no_counterexample(server: &mut mockito::ServerGuard) {
    server
        .mock("POST", "/disprove")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"content":"","lean_messages":{"errors":[],"warnings":[],"infos":[]},"tool_messages":{"errors":[],"warnings":[],"infos":[]},"disproved_theorems":[]}"#)
        .create_async()
        .await;
}

#[tokio::test]
async fn full_pipeline_returns_completed_with_summary() {
    let _guard = env_lock().lock().unwrap_or_else(|err| err.into_inner());
    unsafe { std::env::remove_var("AXLE_API_KEY") };
    let mut server = Server::new_async().await;
    mock_check_ok(&mut server).await;
    mock_sorry2lemma_with_lemmas(&mut server).await;
    mock_disprove_no_counterexample(&mut server).await;

    let mut tmp = NamedTempFile::new().unwrap();
    writeln!(tmp, "import Mathlib\ntheorem foo : True := sorry").unwrap();
    let path = tmp.path().to_str().unwrap().to_string();

    let result = execute_lean_action(&lean_request(&path), &server.url())
        .await
        .unwrap();

    assert_eq!(result.status, ToolActionStatus::Completed);
    assert_eq!(result.tool_family, ToolFamily::LeanExternal);
    assert!(!result.artifact_refs.is_empty());
    let preview = result.stdout_preview.unwrap();
    assert!(preview.contains("check: ok"));
    assert!(preview.contains("lemmas extracted: 1"));
    assert!(preview.contains("disproved: none"));
}

#[tokio::test]
async fn full_pipeline_with_api_key_authenticated_field_is_true() {
    let _guard = env_lock().lock().unwrap_or_else(|err| err.into_inner());
    unsafe { std::env::set_var("AXLE_API_KEY", "test-key") };
    let mut server = Server::new_async().await;
    mock_check_ok(&mut server).await;
    mock_sorry2lemma_with_lemmas(&mut server).await;
    mock_disprove_no_counterexample(&mut server).await;

    let mut tmp = NamedTempFile::new().unwrap();
    writeln!(tmp, "import Mathlib\ntheorem foo : True := sorry").unwrap();
    let path = tmp.path().to_str().unwrap().to_string();

    let result = execute_lean_action(&lean_request(&path), &server.url())
        .await
        .unwrap();

    assert_eq!(result.status, ToolActionStatus::Completed);
    let preview = result.stdout_preview.unwrap();
    assert!(preview.contains("authenticated: true"));

    unsafe { std::env::remove_var("AXLE_API_KEY") };
}

#[tokio::test]
async fn check_failure_returns_failed_and_skips_remaining_steps() {
    let _guard = env_lock().lock().unwrap_or_else(|err| err.into_inner());
    unsafe { std::env::remove_var("AXLE_API_KEY") };
    let mut server = Server::new_async().await;
    server
        .mock("POST", "/check")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"okay":false,"content":"","lean_messages":{"errors":["unknown id 'x'"],"warnings":[],"infos":[]},"tool_messages":{"errors":[],"warnings":[],"infos":[]},"failed_declarations":["foo"]}"#)
        .create_async()
        .await;

    let mut tmp = NamedTempFile::new().unwrap();
    writeln!(tmp, "bad lean code here").unwrap();
    let path = tmp.path().to_str().unwrap().to_string();

    let result = execute_lean_action(&lean_request(&path), &server.url())
        .await
        .unwrap();

    assert_eq!(result.status, ToolActionStatus::Failed);
    let preview = result.stdout_preview.unwrap();
    assert!(preview.contains("check: FAILED"));
    assert!(preview.contains("unknown id 'x'"));
}

#[tokio::test]
async fn missing_lean_file_returns_error() {
    let _guard = env_lock().lock().unwrap_or_else(|err| err.into_inner());
    unsafe { std::env::remove_var("AXLE_API_KEY") };

    let result =
        execute_lean_action(&lean_request("/nonexistent/path.lean"), "http://localhost:9999")
            .await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("failed to read"));
}
