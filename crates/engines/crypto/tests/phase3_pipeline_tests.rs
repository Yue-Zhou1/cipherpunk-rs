use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use audit_agent_core::audit_config::{
    BudgetConfig, OptionalInputsSummary, ResolvedScope, ResolvedSource, SourceOrigin,
};
use audit_agent_core::finding::Framework;
use audit_agent_core::output::{AuditManifest, FindingCounts};
use engine_crypto::zk::halo2::smt_checker::{
    Halo2SmtChecker, Halo2SmtExecutionOutput, Halo2SmtRunner,
};
use engine_crypto::zk::phase3_pipeline::{
    ImageBindingCheckRequest, Phase3PipelineRequest, run_phase3_analysis_and_update_manifest,
};
use engine_crypto::zk::zkvm::diff_tester::{DiffTestRequest, ZkvmBackend, ZkvmDiffTester};
use intake::workspace::WorkspaceAnalyzer;
use sha2::{Digest, Sha256};
use tempfile::tempdir;

#[derive(Clone)]
struct StubRunner {
    statuses: HashMap<String, String>,
}

impl StubRunner {
    fn with_statuses(statuses: &[(&str, &str)]) -> Self {
        Self {
            statuses: statuses
                .iter()
                .map(|(chip, status)| (chip.to_string(), status.to_string()))
                .collect(),
        }
    }
}

#[async_trait]
impl Halo2SmtRunner for StubRunner {
    async fn execute(
        &self,
        smt2_file: &Path,
        _timeout_secs: u64,
    ) -> Result<Halo2SmtExecutionOutput> {
        let query = fs::read_to_string(smt2_file)?;
        let chip = query
            .lines()
            .find_map(|line| line.strip_prefix("; chip: "))
            .unwrap_or("unknown");
        let sat = self
            .statuses
            .get(chip)
            .is_some_and(|status| status == "sat");
        Ok(Halo2SmtExecutionOutput {
            stdout: if sat {
                "sat\n(model\n  (define-fun out_a () Int 0)\n  (define-fun out_b () Int 1)\n)\n"
                    .to_string()
            } else {
                "unsat\n".to_string()
            },
            stderr: String::new(),
            exit_code: 0,
            container_digest: "sha256:z3-phase3".to_string(),
            z3_version: Some("4.13.0".to_string()),
        })
    }
}

fn write_file(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent directory");
    }
    fs::write(path, content).expect("write file");
}

fn budget() -> BudgetConfig {
    BudgetConfig {
        kani_timeout_secs: 300,
        z3_timeout_secs: 30,
        fuzz_duration_secs: 3600,
        madsim_ticks: 100_000,
        max_llm_retries: 3,
        semantic_index_timeout_secs: 5,
    }
}

fn sample_manifest() -> AuditManifest {
    AuditManifest {
        audit_id: "audit-phase3".to_string(),
        agent_version: "0.1.0".to_string(),
        source: ResolvedSource {
            local_path: PathBuf::from("/tmp/repo"),
            origin: SourceOrigin::Local {
                original_path: PathBuf::from("/tmp/repo"),
            },
            commit_hash: "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2".to_string(),
            content_hash: "sha256:content".to_string(),
        },
        started_at: Default::default(),
        completed_at: None,
        scope: ResolvedScope {
            target_crates: vec!["zk".to_string()],
            excluded_crates: vec![],
            build_matrix: vec![],
            detected_frameworks: vec![Framework::Halo2, Framework::SP1],
        },
        tool_versions: HashMap::new(),
        container_digests: HashMap::new(),
        finding_counts: FindingCounts::default(),
        risk_score: 100,
        engines_run: vec!["crypto_zk".to_string()],
        engine_outcomes: vec![],
        coverage: None,
        optional_inputs_used: OptionalInputsSummary {
            spec_provided: false,
            prev_audit_provided: false,
            invariants_count: 0,
            entry_points_count: 0,
            llm_prose_used: false,
        },
    }
}

#[tokio::test]
async fn phase3_pipeline_emits_halo2_and_sp1_findings_with_cdg_artifacts() {
    let dir = tempdir().expect("tempdir");
    write_file(
        &dir.path().join("Cargo.toml"),
        r#"[workspace]
members = ["zk"]
resolver = "2"
"#,
    );
    write_file(
        &dir.path().join("zk/Cargo.toml"),
        r#"[package]
name = "zk"
version = "0.1.0"
edition = "2024"
"#,
    );
    write_file(
        &dir.path().join("zk/src/lib.rs"),
        r#"
pub trait Chip {
    fn configure(meta: &mut Meta);
}

pub struct Meta;
impl Meta {
    pub fn advice_column(&mut self) -> &'static str { "advice_a" }
}

pub struct LooseChip;
impl Chip for LooseChip {
    fn configure(meta: &mut Meta) {
        let advice_a = meta.advice_column();
        let _ = advice_a;
    }
}
"#,
    );

    let workspace = WorkspaceAnalyzer::analyze(dir.path()).expect("analyze workspace");
    let halo2_checker =
        Halo2SmtChecker::new(Arc::new(StubRunner::with_statuses(&[("LooseChip", "sat")])));
    let diff_tester = ZkvmDiffTester::without_sandbox_for_tests();

    let guest_path = dir.path().join("zk/guest.bin");
    fs::write(&guest_path, b"guest-image-v1").expect("guest image");
    let mut hasher = Sha256::new();
    hasher.update(b"different-image");
    fs::write(
        PathBuf::from(&guest_path).with_extension("image_hash"),
        hex::encode(hasher.finalize()),
    )
    .expect("hash sidecar");

    let request = Phase3PipelineRequest {
        diff_tests: vec![DiffTestRequest {
            backend: ZkvmBackend::Sp1,
            boundary_input: "u64::MAX".to_string(),
            native_output: "18446744073709551615".to_string(),
            zkvm_output: "0".to_string(),
        }],
        image_binding_checks: vec![ImageBindingCheckRequest {
            backend: ZkvmBackend::Sp1,
            guest_path: guest_path.clone(),
            crate_name: "zk".to_string(),
            module: "guest".to_string(),
        }],
    };

    let mut manifest = sample_manifest();
    let output = run_phase3_analysis_and_update_manifest(
        &workspace,
        &budget(),
        &halo2_checker,
        &diff_tester,
        &request,
        &mut manifest,
    )
    .await
    .expect("phase3 analysis");

    assert_eq!(output.diff_results.len(), 1);
    assert!(output.diff_results[0].divergence_detected);
    assert!(
        output.tool_versions.contains_key("semantic_backend"),
        "semantic backend should be recorded in tool_versions"
    );
    assert!(
        manifest.tool_versions.contains_key("semantic_backend"),
        "semantic backend should flow into audit manifest tool versions"
    );

    assert!(
        output
            .halo2_cdg_json
            .as_deref()
            .is_some_and(|json| json.contains("LooseChip"))
    );
    assert!(
        output
            .halo2_cdg_dot
            .as_deref()
            .is_some_and(|dot| dot.contains("digraph cdg"))
    );

    assert!(
        output
            .findings
            .iter()
            .any(|f| f.framework == Framework::Halo2)
    );
    assert!(
        output
            .findings
            .iter()
            .any(|f| f.framework == Framework::SP1 && f.title.contains("divergence"))
    );
    assert!(output.findings.iter().any(|f| {
        f.framework == Framework::SP1 && f.title.contains("image hash binding mismatch")
    }));
}
