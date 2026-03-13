use audit_agent_core::finding::VerificationStatus;
use llm::copilot::{ChecklistPlan, CopilotService};

#[tokio::test]
async fn checklist_plan_parses_structured_json_only() {
    let service = CopilotService::with_mock_json(
        r#"{"domains":[{"id":"crypto","rationale":"key material present"}]}"#,
    );
    let plan: ChecklistPlan = service
        .plan_checklists("rust crypto workspace")
        .await
        .expect("checklist plan");
    assert_eq!(plan.domains[0].id, "crypto");
}

#[tokio::test]
async fn candidate_generation_never_returns_verified_status() {
    let service =
        CopilotService::with_mock_json(r#"{"title":"Possible bug","summary":"review me"}"#);
    let candidate = service
        .generate_candidate("hotspot")
        .await
        .expect("candidate");
    assert!(matches!(
        candidate.verification_status,
        VerificationStatus::Unverified { .. }
    ));
}
