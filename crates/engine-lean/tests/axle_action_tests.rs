use audit_agent_core::tooling::{
    ToolActionRequest, ToolActionStatus, ToolBudget, ToolFamily, ToolTarget,
};
use engine_lean::tool_actions::axle::execute_lean_action;
use engine_lean::types::{AxleDisproveRequest, DEFAULT_LEAN_ENV, LeanWorkflowOutput};
use mockito::Server;
use std::io::Write;
use std::sync::{Mutex, OnceLock};
use tempfile::{NamedTempFile, TempDir};

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

struct EnvVarGuard {
    key: &'static str,
    previous_value: Option<String>,
}

impl EnvVarGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let previous_value = std::env::var(key).ok();
        unsafe { std::env::set_var(key, value) };
        Self {
            key,
            previous_value,
        }
    }

    fn remove(key: &'static str) -> Self {
        let previous_value = std::env::var(key).ok();
        unsafe { std::env::remove_var(key) };
        Self {
            key,
            previous_value,
        }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        if let Some(previous) = &self.previous_value {
            unsafe { std::env::set_var(self.key, previous) };
        } else {
            unsafe { std::env::remove_var(self.key) };
        }
    }
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

fn artifact_root() -> TempDir {
    tempfile::tempdir().expect("tempdir")
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
    let _api_key = EnvVarGuard::remove("AXLE_API_KEY");
    let mut server = Server::new_async().await;
    mock_check_ok(&mut server).await;
    mock_sorry2lemma_with_lemmas(&mut server).await;
    mock_disprove_no_counterexample(&mut server).await;

    let artifact_root = artifact_root();
    let mut tmp = NamedTempFile::new().unwrap();
    write!(tmp, "import Mathlib\ntheorem foo : True := sorry").unwrap();
    let path = tmp.path().to_str().unwrap().to_string();
    let request = lean_request(&path);
    let result_path = artifact_root
        .path()
        .join("axle")
        .join(request.target.slug())
        .join("result.json");

    let result = execute_lean_action(&request, &server.url(), artifact_root.path())
        .await
        .unwrap();

    assert_eq!(result.status, ToolActionStatus::Completed);
    assert_eq!(result.tool_family, ToolFamily::LeanExternal);
    assert!(!result.artifact_refs.is_empty());
    let preview = result.stdout_preview.unwrap();
    assert!(preview.contains("check: ok"));
    assert!(preview.contains("lemmas extracted: 1"));
    assert!(preview.contains("disproved: none"));
    assert!(result_path.exists());

    let written: LeanWorkflowOutput =
        serde_json::from_str(&std::fs::read_to_string(&result_path).unwrap()).unwrap();
    assert!(written.check_okay);
    assert_eq!(written.extracted_lemmas, vec!["subgoal_0"]);
    assert!(!written.authenticated);
}

#[tokio::test]
async fn full_pipeline_with_api_key_authenticated_field_is_true() {
    let _guard = env_lock().lock().unwrap_or_else(|err| err.into_inner());
    let _api_key = EnvVarGuard::set("AXLE_API_KEY", "test-key");
    let mut server = Server::new_async().await;
    mock_check_ok(&mut server).await;
    mock_sorry2lemma_with_lemmas(&mut server).await;
    mock_disprove_no_counterexample(&mut server).await;

    let artifact_root = artifact_root();
    let mut tmp = NamedTempFile::new().unwrap();
    write!(tmp, "import Mathlib\ntheorem foo : True := sorry").unwrap();
    let path = tmp.path().to_str().unwrap().to_string();

    let request = lean_request(&path);
    let result = execute_lean_action(&request, &server.url(), artifact_root.path())
        .await
        .unwrap();

    assert_eq!(result.status, ToolActionStatus::Completed);
    let preview = result.stdout_preview.unwrap();
    assert!(preview.contains("authenticated: true"));
}

#[tokio::test]
async fn check_failure_returns_failed_and_skips_remaining_steps() {
    let _guard = env_lock().lock().unwrap_or_else(|err| err.into_inner());
    let _api_key = EnvVarGuard::remove("AXLE_API_KEY");
    let mut server = Server::new_async().await;
    server
        .mock("POST", "/check")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"okay":false,"content":"","lean_messages":{"errors":["unknown id 'x'"],"warnings":[],"infos":[]},"tool_messages":{"errors":[],"warnings":[],"infos":[]},"failed_declarations":["foo"]}"#)
        .create_async()
        .await;
    let sorry2lemma_mock = server
        .mock("POST", "/sorry2lemma")
        .expect(0)
        .create_async()
        .await;
    let disprove_mock = server
        .mock("POST", "/disprove")
        .expect(0)
        .create_async()
        .await;

    let artifact_root = artifact_root();
    let mut tmp = NamedTempFile::new().unwrap();
    writeln!(tmp, "bad lean code here").unwrap();
    let path = tmp.path().to_str().unwrap().to_string();
    let request = lean_request(&path);

    let result = execute_lean_action(&request, &server.url(), artifact_root.path())
        .await
        .unwrap();

    assert_eq!(result.status, ToolActionStatus::Failed);
    let preview = result.stdout_preview.unwrap();
    assert!(preview.contains("check: FAILED"));
    assert!(preview.contains("unknown id 'x'"));
    sorry2lemma_mock.assert_async().await;
    disprove_mock.assert_async().await;
}

#[tokio::test]
async fn empty_lemma_names_use_original_content_and_no_target_names() {
    let _guard = env_lock().lock().unwrap_or_else(|err| err.into_inner());
    let _api_key = EnvVarGuard::remove("AXLE_API_KEY");
    let mut server = Server::new_async().await;
    mock_check_ok(&mut server).await;
    server
        .mock("POST", "/sorry2lemma")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"content":"rewritten content","lean_messages":{"errors":[],"warnings":[],"infos":[]},"lemma_names":[]}"#)
        .create_async()
        .await;

    let lean_source = "import Mathlib\ntheorem foo : True := sorry";
    let expected_disprove_body = serde_json::to_string(&AxleDisproveRequest {
        content: lean_source.to_string(),
        environment: DEFAULT_LEAN_ENV.to_string(),
        names: None,
        timeout_seconds: Some(60.0),
    })
    .unwrap();
    let disprove_mock = server
        .mock("POST", "/disprove")
        .match_body(mockito::Matcher::JsonString(expected_disprove_body))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"content":"","lean_messages":{"errors":[],"warnings":[],"infos":[]},"tool_messages":{"errors":[],"warnings":[],"infos":[]},"disproved_theorems":[]}"#)
        .create_async()
        .await;

    let artifact_root = artifact_root();
    let mut tmp = NamedTempFile::new().unwrap();
    write!(tmp, "{lean_source}").unwrap();
    let path = tmp.path().to_str().unwrap().to_string();
    let request = lean_request(&path);

    let result = execute_lean_action(&request, &server.url(), artifact_root.path())
        .await
        .unwrap();

    assert_eq!(result.status, ToolActionStatus::Completed);
    let preview = result.stdout_preview.unwrap();
    assert!(preview.contains("lemmas extracted: 0"));
    disprove_mock.assert_async().await;
}

#[tokio::test]
async fn missing_lean_file_returns_error() {
    let _guard = env_lock().lock().unwrap_or_else(|err| err.into_inner());
    let _api_key = EnvVarGuard::remove("AXLE_API_KEY");
    let artifact_root = artifact_root();

    let result = execute_lean_action(
        &lean_request("/nonexistent/path.lean"),
        "http://localhost:9999",
        artifact_root.path(),
    )
    .await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("failed to read"));
}
