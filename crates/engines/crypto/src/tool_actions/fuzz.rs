use audit_agent_core::engine::SandboxImage;
use audit_agent_core::tooling::{ToolExecutionPlan, ToolFamily, ToolTarget};

pub fn plan(session_id: &str, target: &ToolTarget) -> ToolExecutionPlan {
    let target_label = target.display_value().to_string();
    let target_slug = target.slug();

    ToolExecutionPlan {
        tool_family: ToolFamily::CargoFuzz,
        image: SandboxImage::Fuzz,
        command: vec![
            "cargo".to_string(),
            "fuzz".to_string(),
            "run".to_string(),
            target_label,
            "--".to_string(),
            "-max_total_time=900".to_string(),
        ],
        artifact_refs: vec![
            format!("{session_id}/tool-runs/cargo-fuzz/{target_slug}/crashes"),
            format!("{session_id}/tool-runs/cargo-fuzz/{target_slug}/coverage.profdata"),
        ],
        rationale: "Explore state space with randomized inputs and persist reproducible corpora"
            .to_string(),
    }
}
