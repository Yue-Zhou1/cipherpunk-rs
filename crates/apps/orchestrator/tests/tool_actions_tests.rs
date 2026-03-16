use orchestrator::{AuditOrchestrator, ToolActionRequest, ToolFamily};

#[tokio::test]
async fn kani_action_creates_job_and_artifact_refs() {
    let orchestrator = AuditOrchestrator::for_tests();
    let result = orchestrator
        .run_tool_action(ToolActionRequest::kani(
            "sess-1",
            "crate::module::target_fn",
        ))
        .await
        .unwrap();
    assert_eq!(result.tool_family, ToolFamily::Kani);
    assert!(!result.artifact_refs.is_empty());
}
