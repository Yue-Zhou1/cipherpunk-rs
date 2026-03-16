use audit_agent_core::tooling::{ToolActionRequest, ToolBudget, ToolFamily, ToolTarget};
use orchestrator::AuditOrchestrator;
use std::io::Write;
use std::sync::{Mutex, OnceLock};
use tempfile::NamedTempFile;

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

#[tokio::test]
async fn lean_external_bypasses_sandbox_and_attempts_axle_call() {
    let _guard = env_lock().lock().unwrap_or_else(|err| err.into_inner());
    let _api_key = EnvVarGuard::remove("AXLE_API_KEY");
    let _api_url = EnvVarGuard::set("AXLE_API_URL", "http://127.0.0.1:1");

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
}
