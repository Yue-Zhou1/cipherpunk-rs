use audit_agent_core::engine::SandboxImage;
use audit_agent_core::tooling::{ToolExecutionPlan, ToolFamily, ToolTarget};

pub fn plan(session_id: &str, target: &ToolTarget) -> ToolExecutionPlan {
    let target_label = target.display_value().to_string();
    let target_slug = target.slug();

    ToolExecutionPlan {
        tool_family: ToolFamily::Kani,
        image: SandboxImage::Kani,
        command: vec![
            "kani".to_string(),
            "check".to_string(),
            "--function".to_string(),
            target_label,
        ],
        artifact_refs: vec![format!(
            "{session_id}/tool-runs/kani/{target_slug}/report.json"
        )],
        rationale: "Model-check critical control-flow and safety assertions".to_string(),
    }
}
