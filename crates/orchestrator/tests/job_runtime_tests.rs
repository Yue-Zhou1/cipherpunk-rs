use audit_agent_core::session::AuditSession;
use orchestrator::{AuditJobKind, AuditOrchestrator};

#[tokio::test]
async fn project_ir_job_is_emitted_when_session_is_created() {
    let orchestrator = AuditOrchestrator::for_tests();
    let session = AuditSession::sample("sess-1");
    let jobs = orchestrator
        .bootstrap_jobs(&session)
        .await
        .expect("bootstrap jobs");
    assert!(
        jobs.iter()
            .any(|job| matches!(job.kind, AuditJobKind::BuildProjectIr))
    );
}

#[tokio::test]
async fn llm_context_is_available_to_non_verifying_jobs() {
    let orchestrator = AuditOrchestrator::for_tests();
    let ctx = orchestrator.test_context();
    assert!(ctx.llm.is_some());
}
