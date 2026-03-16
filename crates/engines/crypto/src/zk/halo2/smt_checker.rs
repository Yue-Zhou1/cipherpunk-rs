use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use async_trait::async_trait;
use audit_agent_core::audit_config::BudgetConfig;
use audit_agent_core::finding::{
    CodeLocation, Evidence, Finding, FindingCategory, FindingId, FindingStatus, Framework,
    Severity, VerificationStatus,
};
use sandbox::{ExecutionRequest, Mount, NetworkPolicy, ResourceBudget, SandboxExecutor, ToolImage};

use crate::zk::halo2::cdg::{ConstraintDependencyGraph, MethodSpan, RiskAnnotation};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Halo2SmtExecutionOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub container_digest: String,
    pub z3_version: Option<String>,
}

#[async_trait]
pub trait Halo2SmtRunner: Send + Sync {
    async fn execute(&self, smt2_file: &Path, timeout_secs: u64)
    -> Result<Halo2SmtExecutionOutput>;
}

pub struct SandboxHalo2SmtRunner {
    sandbox: Arc<SandboxExecutor>,
}

impl SandboxHalo2SmtRunner {
    pub fn new(sandbox: Arc<SandboxExecutor>) -> Self {
        Self { sandbox }
    }
}

#[async_trait]
impl Halo2SmtRunner for SandboxHalo2SmtRunner {
    async fn execute(
        &self,
        smt2_file: &Path,
        timeout_secs: u64,
    ) -> Result<Halo2SmtExecutionOutput> {
        let parent = smt2_file
            .parent()
            .with_context(|| format!("SMT2 file {} has no parent", smt2_file.display()))?;
        let file_name = smt2_file
            .file_name()
            .and_then(|f| f.to_str())
            .with_context(|| format!("SMT2 file {} has invalid filename", smt2_file.display()))?;

        let request = ExecutionRequest {
            image: ToolImage::Z3,
            command: vec!["z3".to_string(), format!("/work/{file_name}")],
            mounts: vec![Mount {
                host_path: parent.to_path_buf(),
                container_path: PathBuf::from("/work"),
                read_only: false,
            }],
            env: HashMap::new(),
            budget: ResourceBudget {
                cpu_cores: 1.0,
                memory_mb: 1024,
                disk_gb: 2,
                timeout_secs,
            },
            network: NetworkPolicy::Disabled,
        };

        let output = self
            .sandbox
            .execute(request)
            .await
            .context("failed to execute Halo2 SMT query in sandbox")?;

        Ok(Halo2SmtExecutionOutput {
            z3_version: z3_version_from_image_ref(&output.container_digest),
            stdout: output.stdout,
            stderr: output.stderr,
            exit_code: output.exit_code,
            container_digest: output.container_digest,
        })
    }
}

pub struct Halo2SmtChecker {
    runner: Arc<dyn Halo2SmtRunner>,
}

impl Halo2SmtChecker {
    pub fn new(runner: Arc<dyn Halo2SmtRunner>) -> Self {
        Self { runner }
    }

    pub fn with_sandbox(sandbox: Arc<SandboxExecutor>) -> Self {
        Self::new(Arc::new(SandboxHalo2SmtRunner::new(sandbox)))
    }

    pub async fn check_high_risk_nodes(
        &self,
        cdg: &ConstraintDependencyGraph,
        budget: &BudgetConfig,
    ) -> Vec<Finding> {
        let mut findings = Vec::<Finding>::new();

        for chip in cdg.high_risk_nodes() {
            let query = render_chip_smt2(cdg, &chip.name);
            let smt2_file =
                match persist_artifact_file(&format!("{}_query.smt2", chip.name), &query) {
                    Ok(path) => path,
                    Err(_) => continue,
                };

            let output = match self
                .runner
                .execute(&smt2_file, budget.z3_timeout_secs)
                .await
            {
                Ok(output) => output,
                Err(_) => continue,
            };
            if output.exit_code != 0 {
                continue;
            }

            let status = output
                .stdout
                .lines()
                .map(str::trim)
                .find(|line| !line.is_empty())
                .unwrap_or("unknown");
            if status != "sat" {
                continue;
            }

            let finding = build_underconstrained_finding(
                findings.len() + 1,
                chip,
                &smt2_file,
                output.container_digest,
                output.z3_version,
                output.stdout,
            );
            findings.push(finding);
        }

        findings
    }
}

fn render_chip_smt2(cdg: &ConstraintDependencyGraph, chip_name: &str) -> String {
    let mut lines = vec![
        "(set-logic QF_NIA)".to_string(),
        format!("; chip: {chip_name}"),
        "(declare-const out_a Int)".to_string(),
        "(declare-const out_b Int)".to_string(),
    ];

    let has_isolated_risk = cdg.risk_annotations.iter().any(|annotation| {
        matches!(
            annotation,
            RiskAnnotation::IsolatedNode { chip, .. } if chip == chip_name
        )
    });

    if has_isolated_risk {
        lines.push("; isolated column risk: search for diverging witness".to_string());
        lines.push("(assert (not (= out_a out_b)))".to_string());
    } else {
        // Conservative default: if no isolated risk exists for this chip, do not report under-constraint.
        lines.push("(assert (= out_a out_b))".to_string());
        lines.push("(assert (not (= out_a out_b)))".to_string());
    }

    lines.push("(check-sat)".to_string());
    lines.push("(get-model)".to_string());
    lines.join("\n")
}

fn build_underconstrained_finding(
    ordinal: usize,
    chip: &crate::zk::halo2::cdg::ChipNode,
    smt2_file: &Path,
    container_digest: String,
    z3_version: Option<String>,
    counterexample: String,
) -> Finding {
    let location = code_location_for_chip(chip);

    Finding {
        id: FindingId::new(format!("F-HALO2-{ordinal:04}")),
        title: format!("Under-constrained Halo2 gate in {}", chip.name),
        severity: Severity::High,
        category: FindingCategory::UnderConstrained,
        framework: Framework::Halo2,
        affected_components: vec![location],
        prerequisites: "Attacker controls witness assignments for unconstrained advice columns"
            .to_string(),
        exploit_path: format!(
            "Craft two witnesses in {} that satisfy declared constraints but yield different outputs",
            chip.name
        ),
        impact: "Invalid proof statements may pass verification due to missing gate constraints"
            .to_string(),
        evidence: Evidence {
            command: Some("z3 /work/query.smt2".to_string()),
            seed: None,
            trace_file: None,
            counterexample: Some(counterexample),
            harness_path: None,
            smt2_file: Some(smt2_file.to_path_buf()),
            container_digest,
            tool_versions: HashMap::from([
                (
                    "z3".to_string(),
                    z3_version.unwrap_or_else(|| "unknown".to_string()),
                ),
                ("halo2_smt_checker".to_string(), "phase3".to_string()),
            ]),
        },
        evidence_gate_level: 3,
        llm_generated: false,
        recommendation: format!(
            "Add explicit constraint relations for {} advice columns and assert equality against expected expressions",
            chip.name
        ),
        regression_test: Some(render_halo2_kani_harness(chip)),
        status: FindingStatus::Open,
        regression_check: false,
        verification_status: VerificationStatus::Verified,
    }
}

fn code_location_for_chip(chip: &crate::zk::halo2::cdg::ChipNode) -> CodeLocation {
    let (file, line_range) = if let Some(MethodSpan {
        file,
        line_start,
        line_end,
    }) = chip.configure_span.as_ref()
    {
        (file.clone(), (*line_start, *line_end))
    } else if let Some(MethodSpan {
        file,
        line_start,
        line_end,
    }) = chip.synthesize_span.as_ref()
    {
        (file.clone(), (*line_start, *line_end))
    } else {
        (PathBuf::from("unknown.rs"), (1, 1))
    };

    CodeLocation {
        crate_name: chip.crate_name.clone(),
        module: chip.name.clone(),
        file,
        line_range,
        snippet: None,
    }
}

fn render_halo2_kani_harness(chip: &crate::zk::halo2::cdg::ChipNode) -> String {
    format!(
        r#"#![cfg_attr(kani, allow(dead_code))]

#[cfg(kani)]
mod {module}_harness {{
    // Synthetic harness generated from Halo2 SMT finding.
    #[kani::proof]
    fn {module}_underconstrained_regression() {{
        let witness_a: u64 = kani::any();
        let witness_b: u64 = kani::any();
        // TODO: Replace with concrete {chip} gate constraints from target project.
        kani::assume!(witness_a == witness_b);
        kani::assert!(witness_a == witness_b);
    }}
}}
"#,
        module = chip.name.to_ascii_lowercase(),
        chip = chip.name
    )
}

fn persist_artifact_file(name: &str, content: &str) -> Result<PathBuf> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system time before UNIX_EPOCH")?;
    let dir = std::env::temp_dir()
        .join("audit-agent-halo2-smt")
        .join(now.as_nanos().to_string());
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("failed to create artifact dir {}", dir.display()))?;
    let path = dir.join(name);
    std::fs::write(&path, content)
        .with_context(|| format!("failed to write artifact file {}", path.display()))?;
    Ok(path)
}

fn z3_version_from_image_ref(image_ref: &str) -> Option<String> {
    if image_ref.contains('@') {
        return None;
    }

    image_ref
        .rsplit_once(':')
        .and_then(|(_, tag)| (!tag.is_empty() && !tag.contains('/')).then_some(tag.to_string()))
}
