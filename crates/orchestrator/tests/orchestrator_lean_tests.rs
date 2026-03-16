use audit_agent_core::tooling::{ToolActionRequest, ToolBudget, ToolFamily, ToolTarget};
use orchestrator::AuditOrchestrator;
use std::io::Write;
use std::sync::{Mutex, OnceLock};
use tempfile::NamedTempFile;

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[tokio::test]
async fn lean_external_bypasses_sandbox_and_attempts_axle_call() {
    let _guard = env_lock().lock().expect("env lock");
    unsafe { std::env::remove_var("AXLE_API_KEY") };
    unsafe { std::env::set_var("AXLE_API_URL", "http://127.0.0.1:1") };

    let orchestrator = AuditOrchestrator::for_tests();

    let mut tmp = NamedTempFile::new().unwrap();
    writeln!(tmp, "import Mathlib\ntheorem foo : True := sorry").unwrap();
    let path = tmp.path().to_str().unwrap().to_string();

    let request = ToolActionRequest {
        session_id: "sess-orchestrator-lean-test".to_string(),
        workspace_root: None,
        tool_family: ToolFamily::LeanExternal,
        target: ToolTarget::File { path },
        budget: ToolBudget::default(),
    };

    let result = orchestrator.run_tool_action(request).await;

    assert!(
        result.is_err(),
        "expected HTTP connection error from AXLE path, got Ok - sandbox may have intercepted"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("AXLE") || msg.contains("connection") || msg.contains("refused"),
        "unexpected error: {msg}"
    );

    unsafe { std::env::remove_var("AXLE_API_URL") };
}
