use audit_agent_core::engine::SandboxImage;
use audit_agent_core::tooling::{ToolExecutionPlan, ToolFamily, ToolTarget};

pub fn plan(session_id: &str, target: &ToolTarget) -> ToolExecutionPlan {
    let target_label = target.display_value().to_string();
    let target_slug = target.slug();

    ToolExecutionPlan {
        tool_family: ToolFamily::Z3,
        image: SandboxImage::Z3,
        command: vec![
            "z3".to_string(),
            "-smt2".to_string(),
            format!("{target_label}.smt2"),
        ],
        artifact_refs: vec![format!("{session_id}/tool-runs/z3/{target_slug}/model.txt")],
        rationale: "Discharge symbolic constraints and produce satisfiable counterexamples"
            .to_string(),
    }
}

pub fn plan_circom(session_id: &str, target: &ToolTarget) -> ToolExecutionPlan {
    let target_label = target.display_value().to_string();
    let target_slug = target.slug();

    ToolExecutionPlan {
        tool_family: ToolFamily::CircomZ3,
        image: SandboxImage::Z3,
        command: vec![
            "z3".to_string(),
            "-smt2".to_string(),
            format!("{target_label}.constraints.smt2"),
        ],
        artifact_refs: vec![format!(
            "{session_id}/tool-runs/circom-z3/{target_slug}/counterexample.json"
        )],
        rationale: "Validate Circom constraint consistency and under-constraint risk".to_string(),
    }
}
