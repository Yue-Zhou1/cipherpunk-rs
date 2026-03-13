use audit_agent_core::engine::SandboxImage;
use audit_agent_core::tooling::{ToolExecutionPlan, ToolFamily, ToolTarget};

pub fn plan(session_id: &str, target: &ToolTarget) -> ToolExecutionPlan {
    let target_slug = target.slug();

    ToolExecutionPlan {
        tool_family: ToolFamily::MadSim,
        image: SandboxImage::MadSim,
        command: vec![
            "cargo".to_string(),
            "madsim".to_string(),
            "test".to_string(),
            "--".to_string(),
            "--nocapture".to_string(),
        ],
        artifact_refs: vec![
            format!("{session_id}/tool-runs/madsim/{target_slug}/trace.log"),
            format!("{session_id}/tool-runs/madsim/{target_slug}/invariants.json"),
        ],
        rationale: "Exercise distributed schedules and surface safety/liveness regressions"
            .to_string(),
    }
}
