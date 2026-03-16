use std::path::PathBuf;

use anyhow::Result;
use audit_agent_core::engine::{SandboxRequest, SandboxResult, SandboxRunner};
use orchestrator::{AuditOrchestrator, ToolActionRequest, ToolFamily};

#[derive(Debug, Default)]
struct RemoteLikeSandbox;

#[async_trait::async_trait]
impl SandboxRunner for RemoteLikeSandbox {
    async fn execute(&self, _request: SandboxRequest) -> Result<SandboxResult> {
        Ok(SandboxResult {
            exit_code: 0,
            stdout: "remote worker ok".to_string(),
            stderr: String::new(),
            artifacts: vec![PathBuf::from("/artifacts/report.json")],
            container_digest: "sha256:remote-worker".to_string(),
            duration_ms: 5,
            resource_usage: Default::default(),
        })
    }
}

#[tokio::test]
async fn end_to_end_v3_tool_action_runs_with_remote_worker_backend() {
    let orchestrator = AuditOrchestrator::for_tests()
        .with_sandbox(std::sync::Arc::new(RemoteLikeSandbox::default()));
    let result = orchestrator
        .run_tool_action(ToolActionRequest::kani("sess-v3", "crate::module::prove"))
        .await
        .expect("tool action run");

    assert_eq!(result.tool_family, ToolFamily::Kani);
    assert!(
        result
            .artifact_refs
            .iter()
            .any(|artifact| artifact.contains("report")),
        "expected artifact refs to include a report path"
    );
    assert!(
        result
            .stdout_preview
            .as_deref()
            .is_some_and(|preview| preview.contains("remote worker ok")),
        "expected remote sandbox stdout to propagate into action preview"
    );
}
