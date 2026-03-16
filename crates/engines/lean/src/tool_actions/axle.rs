use anyhow::{Context, Result};
use audit_agent_core::engine::SandboxImage;
use audit_agent_core::tooling::{
    ToolActionRequest, ToolActionResult, ToolActionStatus, ToolExecutionPlan, ToolFamily,
    ToolTarget,
};
use chrono::Utc;
use std::path::Path;

use crate::client::AxleClient;
use crate::types::{
    AxleCheckRequest, AxleDisproveRequest, AxleSorry2LemmaRequest, DEFAULT_LEAN_ENV,
    LeanWorkflowOutput,
};

pub async fn execute_lean_action(
    request: &ToolActionRequest,
    axle_base_url: &str,
    artifact_root: &Path,
) -> Result<ToolActionResult> {
    let lean_path = request.target.display_value();
    let target_slug = request.target.slug();
    let lean_content = std::fs::read_to_string(lean_path)
        .with_context(|| format!("failed to read Lean file: {lean_path}"))?;

    // AXLE calls are direct HTTP requests and bypass the sandbox, so
    // `budget.allow_network` does not apply in this execution path.
    let client = AxleClient::from_env(axle_base_url.to_string());
    let authenticated = client.has_api_key();

    let timeout_per_step = (request.budget.timeout_secs as f64) / 3.0;

    let check = client
        .check(&AxleCheckRequest {
            content: lean_content.clone(),
            environment: DEFAULT_LEAN_ENV.to_string(),
            timeout_seconds: Some(timeout_per_step),
        })
        .await?;

    if !check.okay {
        let output = LeanWorkflowOutput {
            check_okay: false,
            check_errors: check.lean_messages.errors.clone(),
            extracted_lemmas: vec![],
            disproved_theorems: vec![],
            lean_environment: DEFAULT_LEAN_ENV.to_string(),
            authenticated,
        };
        let summary = format!(
            "check: FAILED\nauthenticated: {authenticated}\nerrors: {}",
            check.lean_messages.errors.join("; ")
        );
        return build_result(
            request,
            ToolActionStatus::Failed,
            &target_slug,
            artifact_root,
            &output,
            summary,
        );
    }

    let sorry = client
        .sorry2lemma(&AxleSorry2LemmaRequest {
            content: lean_content.clone(),
            environment: DEFAULT_LEAN_ENV.to_string(),
            extract_sorries: Some(true),
            extract_errors: Some(true),
            timeout_seconds: Some(timeout_per_step),
        })
        .await?;

    let (disprove_content, disprove_names) = if sorry.lemma_names.is_empty() {
        (lean_content, None)
    } else {
        (sorry.content.clone(), Some(sorry.lemma_names.clone()))
    };

    let disprove = client
        .disprove(&AxleDisproveRequest {
            content: disprove_content,
            environment: DEFAULT_LEAN_ENV.to_string(),
            names: disprove_names,
            timeout_seconds: Some(timeout_per_step),
        })
        .await?;

    let output = LeanWorkflowOutput {
        check_okay: true,
        check_errors: vec![],
        extracted_lemmas: sorry.lemma_names,
        disproved_theorems: disprove.disproved_theorems.clone(),
        lean_environment: DEFAULT_LEAN_ENV.to_string(),
        authenticated,
    };

    let summary = format!(
        "check: ok\nauthenticated: {authenticated}\nlemmas extracted: {}\ndisproved: {}",
        output.extracted_lemmas.len(),
        if output.disproved_theorems.is_empty() {
            "none".to_string()
        } else {
            output.disproved_theorems.join(", ")
        }
    );
    build_result(
        request,
        ToolActionStatus::Completed,
        &target_slug,
        artifact_root,
        &output,
        summary,
    )
}

fn build_result(
    request: &ToolActionRequest,
    status: ToolActionStatus,
    target_slug: &str,
    artifact_root: &Path,
    output: &LeanWorkflowOutput,
    summary: String,
) -> Result<ToolActionResult> {
    let artifact_ref = format!(
        "{}/tool-runs/axle/{target_slug}/result.json",
        request.session_id
    );
    let result_path = artifact_root
        .join("axle")
        .join(target_slug)
        .join("result.json");
    if let Some(parent) = result_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create artifact dir {}", parent.display()))?;
    }
    let payload = serde_json::to_vec_pretty(output).context("failed to serialize AXLE output")?;
    std::fs::write(&result_path, payload)
        .with_context(|| format!("failed to write AXLE result {}", result_path.display()))?;

    let preview = summary[..summary.len().min(1024)].to_string();
    Ok(ToolActionResult {
        action_id: format!("axle-{}", Utc::now().timestamp_micros()),
        session_id: request.session_id.clone(),
        tool_family: ToolFamily::LeanExternal,
        target: request.target.clone(),
        command: vec![
            "axle".to_string(),
            "check+sorry2lemma+disprove".to_string(),
            request.target.display_value().to_string(),
        ],
        artifact_refs: vec![artifact_ref],
        rationale: "AXLE: validate Lean file, decompose stubs, search for counterexamples"
            .to_string(),
        status,
        stdout_preview: Some(preview),
        stderr_preview: None,
    })
}

pub fn sentinel_plan(session_id: &str, target: &ToolTarget) -> ToolExecutionPlan {
    ToolExecutionPlan {
        tool_family: ToolFamily::LeanExternal,
        image: SandboxImage::Custom("axle-remote".to_string()),
        command: vec!["axle".to_string(), target.display_value().to_string()],
        artifact_refs: vec![format!(
            "{session_id}/tool-runs/axle/{}/result.json",
            target.slug()
        )],
        rationale: "AXLE remote API - dispatched directly, not via sandbox".to_string(),
    }
}
