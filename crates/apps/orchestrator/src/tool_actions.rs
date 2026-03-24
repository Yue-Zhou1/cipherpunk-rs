use std::collections::HashMap;
use std::path::Path;

use audit_agent_core::engine::{
    SandboxBudget, SandboxImage, SandboxMount, SandboxNetworkPolicy, SandboxRequest,
};
use audit_agent_core::tooling::{
    ToolActionRequest, ToolBudget, ToolExecutionPlan, ToolFamily, ToolTarget,
};

pub fn plan_tool_action(request: &ToolActionRequest) -> ToolExecutionPlan {
    match request.tool_family {
        ToolFamily::Kani => {
            engine_crypto::tool_actions::kani::plan(&request.session_id, &request.target)
        }
        ToolFamily::Z3 => {
            engine_crypto::tool_actions::z3::plan(&request.session_id, &request.target)
        }
        ToolFamily::CargoFuzz => {
            engine_crypto::tool_actions::fuzz::plan(&request.session_id, &request.target)
        }
        ToolFamily::MadSim => {
            engine_distributed::tool_actions::madsim::plan(&request.session_id, &request.target)
        }
        ToolFamily::Chaos => {
            engine_distributed::tool_actions::chaos::plan(&request.session_id, &request.target)
        }
        ToolFamily::CircomZ3 => {
            engine_crypto::tool_actions::z3::plan_circom(&request.session_id, &request.target)
        }
        ToolFamily::Research => external_plan(
            "research-adapter",
            ToolFamily::Research,
            &request.session_id,
            &request.target,
            "Bounded research action against allowlisted advisory sources",
        ),
        ToolFamily::CairoExternal => external_plan(
            "cairo-external-adapter",
            ToolFamily::CairoExternal,
            &request.session_id,
            &request.target,
            "External Cairo adapter slot (explicitly configured by analyst policy)",
        ),
        ToolFamily::LeanExternal => {
            engine_lean::tool_actions::axle::sentinel_plan(&request.session_id, &request.target)
        }
    }
}

pub fn sandbox_request(
    plan: &ToolExecutionPlan,
    budget: &ToolBudget,
    workspace_root: &Path,
    artifact_root: &Path,
) -> SandboxRequest {
    let workspace_read_only = !matches!(
        plan.tool_family,
        ToolFamily::CargoFuzz | ToolFamily::MadSim | ToolFamily::Kani
    );

    SandboxRequest {
        image: plan.image.clone(),
        command: plan.command.clone(),
        mounts: vec![
            SandboxMount {
                host_path: workspace_root.to_path_buf(),
                container_path: "/workspace".into(),
                read_only: workspace_read_only,
            },
            SandboxMount {
                host_path: artifact_root.to_path_buf(),
                container_path: "/artifacts".into(),
                read_only: false,
            },
        ],
        env: HashMap::from([
            ("WORKSPACE_ROOT".to_string(), "/workspace".to_string()),
            ("ARTIFACT_ROOT".to_string(), "/artifacts".to_string()),
        ]),
        budget: SandboxBudget {
            cpu_cores: budget.cpu_cores,
            memory_mb: budget.memory_mb,
            disk_gb: budget.disk_gb,
            timeout_secs: budget.timeout_secs,
        },
        network: if budget.allow_network {
            SandboxNetworkPolicy::Allowlist(vec![])
        } else {
            SandboxNetworkPolicy::Disabled
        },
    }
}

fn external_plan(
    adapter_cmd: &str,
    tool_family: ToolFamily,
    session_id: &str,
    target: &ToolTarget,
    rationale: &str,
) -> ToolExecutionPlan {
    let target_value = target.display_value().to_string();
    let target_slug = target.slug();
    ToolExecutionPlan {
        tool_family,
        image: SandboxImage::Custom("audit-agent/external-adapter:0.1.0".to_string()),
        command: vec![
            adapter_cmd.to_string(),
            "--target".to_string(),
            target_value,
        ],
        artifact_refs: vec![format!(
            "{session_id}/tool-runs/{adapter_cmd}/{target_slug}/output.json"
        )],
        rationale: rationale.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use audit_agent_core::tooling::ToolFamily;

    use super::*;

    fn sample_plan(tool_family: ToolFamily) -> ToolExecutionPlan {
        ToolExecutionPlan {
            tool_family,
            image: SandboxImage::Kani,
            command: vec!["kani".to_string(), "--version".to_string()],
            artifact_refs: vec![],
            rationale: "test plan".to_string(),
        }
    }

    #[test]
    fn workspace_mount_is_read_only_for_non_cargo_tools() {
        let request = sandbox_request(
            &sample_plan(ToolFamily::Z3),
            &ToolBudget::default(),
            &PathBuf::from("/tmp/workspace"),
            &PathBuf::from("/tmp/artifacts"),
        );
        assert_eq!(request.mounts.len(), 2);
        assert!(request.mounts[0].read_only, "workspace should be read-only");
        assert!(
            !request.mounts[1].read_only,
            "artifact mount should remain writable"
        );
    }

    #[test]
    fn workspace_mount_is_writable_for_cargo_based_tools() {
        for family in [ToolFamily::CargoFuzz, ToolFamily::MadSim, ToolFamily::Kani] {
            let request = sandbox_request(
                &sample_plan(family),
                &ToolBudget::default(),
                &PathBuf::from("/tmp/workspace"),
                &PathBuf::from("/tmp/artifacts"),
            );
            assert_eq!(request.mounts.len(), 2);
            assert!(
                !request.mounts[0].read_only,
                "cargo-backed tool family should have writable workspace"
            );
        }
    }
}
