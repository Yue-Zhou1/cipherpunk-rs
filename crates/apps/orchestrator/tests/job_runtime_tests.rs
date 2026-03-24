use async_trait::async_trait;
use audit_agent_core::engine::{AuditContext, AuditEngine};
use audit_agent_core::finding::Finding;
use audit_agent_core::session::AuditSession;
use orchestrator::{AuditJobKind, AuditOrchestrator};

struct NoopEngine;

#[async_trait]
impl AuditEngine for NoopEngine {
    fn name(&self) -> &str {
        "noop-engine"
    }

    async fn analyze(&self, _ctx: &AuditContext) -> anyhow::Result<Vec<Finding>> {
        Ok(vec![])
    }

    async fn supports(&self, _ctx: &AuditContext) -> bool {
        true
    }
}

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

#[tokio::test]
async fn bootstrap_jobs_include_per_engine_entries_before_domain_checklists() {
    let orchestrator = AuditOrchestrator::for_tests().with_engines(vec![Box::new(NoopEngine)]);
    let mut session = AuditSession::sample("sess-2");
    session.selected_domains = vec!["crypto".to_string()];

    let jobs = orchestrator
        .bootstrap_jobs(&session)
        .await
        .expect("bootstrap jobs");

    let engine_index = jobs
        .iter()
        .position(|job| {
            matches!(
                &job.kind,
                AuditJobKind::RunEngine { engine_name } if engine_name == "noop-engine"
            )
        })
        .expect("missing run-engine job");

    let domain_index = jobs
        .iter()
        .position(|job| matches!(&job.kind, AuditJobKind::RunDomainChecklist { .. }))
        .expect("missing domain checklist job");

    assert!(engine_index < domain_index);
}
