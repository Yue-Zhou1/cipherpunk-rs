use audit_agent_core::engine::SandboxImage;
use audit_agent_core::tooling::{ToolExecutionPlan, ToolFamily, ToolTarget};

pub fn plan(session_id: &str, target: &ToolTarget) -> ToolExecutionPlan {
    let target_label = target.display_value().to_string();
    let target_slug = target.slug();

    ToolExecutionPlan {
        tool_family: ToolFamily::Chaos,
        image: SandboxImage::Chaos,
        command: vec![
            "chaos-runner".to_string(),
            "--scenario".to_string(),
            format!("{target_label}.yaml"),
        ],
        artifact_refs: vec![
            format!("{session_id}/tool-runs/chaos/{target_slug}/timeline.json"),
            format!("{session_id}/tool-runs/chaos/{target_slug}/violations.log"),
        ],
        rationale: "Inject partitions, drops, and byzantine schedules under replayable seeds"
            .to_string(),
    }
}
