use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Result;
use audit_agent_core::audit_config::BudgetConfig;
use audit_agent_core::finding::{
    CodeLocation, Evidence, Finding, FindingCategory, FindingId, FindingStatus, Framework,
    Severity, VerificationStatus,
};
use audit_agent_core::output::AuditManifest;
use audit_agent_core::workspace::CargoWorkspace;

use crate::semantic::ra_client::{SemanticBackend, SemanticIndex};
use crate::zk::halo2::cdg::ConstraintDependencyGraph;
use crate::zk::halo2::smt_checker::Halo2SmtChecker;
use crate::zk::zkvm::diff_tester::{DiffTestRequest, DiffTestResult, ZkvmBackend, ZkvmDiffTester};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageBindingCheckRequest {
    pub backend: ZkvmBackend,
    pub guest_path: PathBuf,
    pub crate_name: String,
    pub module: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Phase3PipelineRequest {
    pub diff_tests: Vec<DiffTestRequest>,
    pub image_binding_checks: Vec<ImageBindingCheckRequest>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Phase3AnalysisOutput {
    pub semantic_backend: SemanticBackend,
    pub tool_versions: HashMap<String, String>,
    pub halo2_cdg_dot: Option<String>,
    pub halo2_cdg_json: Option<String>,
    pub findings: Vec<Finding>,
    pub diff_results: Vec<DiffTestResult>,
}

pub async fn run_phase3_analysis(
    workspace: &CargoWorkspace,
    budget: &BudgetConfig,
    halo2_checker: &Halo2SmtChecker,
    diff_tester: &ZkvmDiffTester,
    request: &Phase3PipelineRequest,
) -> Result<Phase3AnalysisOutput> {
    let semantic_index = SemanticIndex::build(workspace, budget).await?;
    let mut findings = Vec::<Finding>::new();
    let mut diff_results = Vec::<DiffTestResult>::new();
    let mut tool_versions = HashMap::<String, String>::new();
    semantic_index.record_backend_tool_version(&mut tool_versions);

    let mut halo2_cdg_dot = None;
    let mut halo2_cdg_json = None;

    if !semantic_index
        .find_trait_impls("Chip", "configure")
        .is_empty()
    {
        let cdg = ConstraintDependencyGraph::build(&semantic_index)?;
        halo2_cdg_dot = Some(cdg.to_dot());
        halo2_cdg_json = Some(cdg.to_json());

        let halo2_findings = halo2_checker.check_high_risk_nodes(&cdg, budget).await;
        findings.extend(halo2_findings);
    }

    for req in &request.diff_tests {
        let result = diff_tester.run(req.clone()).await?;
        if result.divergence_detected {
            findings.push(zkvm_divergence_finding(findings.len() + 1, &result));
        }
        diff_results.push(result);
    }

    for check in &request.image_binding_checks {
        let hash_ok = diff_tester
            .verify_image_hash_binding(&check.guest_path)
            .await?;
        if !hash_ok {
            findings.push(image_hash_binding_finding(findings.len() + 1, check));
        }
    }

    Ok(Phase3AnalysisOutput {
        semantic_backend: semantic_index.backend,
        tool_versions,
        halo2_cdg_dot,
        halo2_cdg_json,
        findings,
        diff_results,
    })
}

pub async fn run_phase3_analysis_and_update_manifest(
    workspace: &CargoWorkspace,
    budget: &BudgetConfig,
    halo2_checker: &Halo2SmtChecker,
    diff_tester: &ZkvmDiffTester,
    request: &Phase3PipelineRequest,
    manifest: &mut AuditManifest,
) -> Result<Phase3AnalysisOutput> {
    let output =
        run_phase3_analysis(workspace, budget, halo2_checker, diff_tester, request).await?;
    merge_phase3_tool_versions_into_manifest(manifest, &output);
    Ok(output)
}

pub fn merge_phase3_tool_versions_into_manifest(
    manifest: &mut AuditManifest,
    output: &Phase3AnalysisOutput,
) {
    manifest.tool_versions.extend(output.tool_versions.clone());
}

fn zkvm_divergence_finding(ordinal: usize, result: &DiffTestResult) -> Finding {
    let framework = match result.backend {
        ZkvmBackend::Sp1 => Framework::SP1,
        ZkvmBackend::Risc0 => Framework::RISC0,
    };

    Finding {
        id: FindingId::new(format!("F-ZKVM-{ordinal:04}")),
        title: format!(
            "zkVM output divergence on boundary input {}",
            result.boundary_input
        ),
        severity: Severity::High,
        category: FindingCategory::SpecMismatch,
        framework,
        affected_components: vec![CodeLocation {
            crate_name: "zkvm".to_string(),
            module: "diff_tester".to_string(),
            file: PathBuf::from("unknown.rs"),
            line_range: (1, 1),
            snippet: None,
        }],
        prerequisites: "Boundary input reaches divergent execution path".to_string(),
        exploit_path: result.summary.clone(),
        impact: "Native and zkVM execution semantics diverge, weakening proof confidence"
            .to_string(),
        evidence: Evidence {
            command: None,
            seed: None,
            trace_file: None,
            counterexample: Some(format!(
                "native={}, zkvm={}",
                result.native_output, result.zkvm_output
            )),
            harness_path: None,
            smt2_file: None,
            container_digest: "n/a".to_string(),
            tool_versions: HashMap::from([("zkvm_diff_tester".to_string(), "phase3".to_string())]),
        },
        evidence_gate_level: 2,
        llm_generated: false,
        recommendation:
            "Align host and guest arithmetic/input normalization at boundary conditions".to_string(),
        regression_test: None,
        status: FindingStatus::Open,
        regression_check: false,
        verification_status: VerificationStatus::Verified,
    }
}

fn image_hash_binding_finding(ordinal: usize, check: &ImageBindingCheckRequest) -> Finding {
    let framework = match check.backend {
        ZkvmBackend::Sp1 => Framework::SP1,
        ZkvmBackend::Risc0 => Framework::RISC0,
    };

    Finding {
        id: FindingId::new(format!("F-ZKVM-BIND-{ordinal:04}")),
        title: "zkVM image hash binding mismatch".to_string(),
        severity: Severity::High,
        category: FindingCategory::SpecMismatch,
        framework,
        affected_components: vec![CodeLocation {
            crate_name: check.crate_name.clone(),
            module: check.module.clone(),
            file: check.guest_path.clone(),
            line_range: (1, 1),
            snippet: None,
        }],
        prerequisites: "Guest image hash is checked against stale or mismatched binding"
            .to_string(),
        exploit_path: format!(
            "Guest image {} did not match expected hash binding",
            check.guest_path.display()
        ),
        impact: "Verifier may accept proofs tied to unintended guest image".to_string(),
        evidence: Evidence {
            command: None,
            seed: None,
            trace_file: None,
            counterexample: None,
            harness_path: None,
            smt2_file: None,
            container_digest: "n/a".to_string(),
            tool_versions: HashMap::from([("zkvm_diff_tester".to_string(), "phase3".to_string())]),
        },
        evidence_gate_level: 2,
        llm_generated: false,
        recommendation:
            "Regenerate and pin image hash binding from exact guest binary used for proving"
                .to_string(),
        regression_test: None,
        status: FindingStatus::Open,
        regression_check: false,
        verification_status: VerificationStatus::Verified,
    }
}
