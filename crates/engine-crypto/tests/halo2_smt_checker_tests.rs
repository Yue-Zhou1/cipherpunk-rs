use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use audit_agent_core::audit_config::BudgetConfig;
use audit_agent_core::finding::{FindingCategory, Framework, Severity, VerificationStatus};
use engine_crypto::zk::halo2::cdg::{
    ChipNode, ConstraintDependencyGraph, MethodSpan, RiskAnnotation,
};
use engine_crypto::zk::halo2::smt_checker::{
    Halo2SmtChecker, Halo2SmtExecutionOutput, Halo2SmtRunner,
};

#[derive(Clone)]
struct StubRunner {
    statuses: HashMap<String, String>,
    z3_version: String,
}

impl StubRunner {
    fn with_statuses(statuses: &[(&str, &str)]) -> Self {
        Self {
            statuses: statuses
                .iter()
                .map(|(chip, status)| (chip.to_string(), status.to_string()))
                .collect(),
            z3_version: "4.13.0".to_string(),
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
        let status = self
            .statuses
            .get(chip)
            .map(String::as_str)
            .unwrap_or("unsat");

        Ok(Halo2SmtExecutionOutput {
            stdout: format!(
                "{status}\n(model\n  (define-fun out_a () Int 0)\n  (define-fun out_b () Int 1)\n)\n"
            ),
            stderr: String::new(),
            exit_code: 0,
            container_digest: "sha256:z3-halo2".to_string(),
            z3_version: Some(self.z3_version.clone()),
        })
    }
}

fn budget() -> BudgetConfig {
    BudgetConfig {
        kani_timeout_secs: 300,
        z3_timeout_secs: 30,
        fuzz_duration_secs: 3600,
        madsim_ticks: 100_000,
        max_llm_retries: 3,
        semantic_index_timeout_secs: 120,
    }
}

fn synthetic_cdg() -> ConstraintDependencyGraph {
    ConstraintDependencyGraph {
        chips: vec![
            ChipNode {
                name: "LooseChip".to_string(),
                crate_name: "zk".to_string(),
                configure_span: Some(MethodSpan {
                    file: PathBuf::from("src/loose_chip.rs"),
                    line_start: 12,
                    line_end: 30,
                }),
                synthesize_span: None,
                columns: vec!["advice_a".to_string()],
            },
            ChipNode {
                name: "RangeCheckChip".to_string(),
                crate_name: "zk".to_string(),
                configure_span: Some(MethodSpan {
                    file: PathBuf::from("src/range_chip.rs"),
                    line_start: 3,
                    line_end: 18,
                }),
                synthesize_span: None,
                columns: vec!["advice_range".to_string()],
            },
        ],
        edges: vec![],
        risk_annotations: vec![RiskAnnotation::IsolatedNode {
            chip: "LooseChip".to_string(),
            column: "advice_a".to_string(),
        }],
    }
}

#[tokio::test]
async fn finds_under_constrained_gate_on_synthetic_halo2_fixture() {
    let checker = Halo2SmtChecker::new(Arc::new(StubRunner::with_statuses(&[
        ("LooseChip", "sat"),
        ("RangeCheckChip", "unsat"),
    ])));

    let findings = checker
        .check_high_risk_nodes(&synthetic_cdg(), &budget())
        .await;

    assert_eq!(findings.len(), 1);
    let finding = &findings[0];
    assert_eq!(finding.framework, Framework::Halo2);
    assert_eq!(finding.category, FindingCategory::UnderConstrained);
    assert_eq!(finding.severity, Severity::High);
    assert_eq!(finding.verification_status, VerificationStatus::Verified);
    assert_eq!(finding.affected_components[0].crate_name, "zk");
    assert_eq!(finding.affected_components[0].line_range, (12, 30));
    assert!(
        finding
            .evidence
            .counterexample
            .as_ref()
            .is_some_and(|v| v.contains("define-fun out_a"))
    );
    assert!(finding.evidence.smt2_file.is_some());
    assert_eq!(
        finding.evidence.tool_versions.get("z3"),
        Some(&"4.13.0".to_string())
    );
    assert!(
        finding
            .regression_test
            .as_ref()
            .is_some_and(|h| h.contains("kani::proof")),
        "halo2 finding should include generated kani harness"
    );
}

#[tokio::test]
async fn constrained_result_produces_no_finding() {
    let checker = Halo2SmtChecker::new(Arc::new(StubRunner::with_statuses(&[(
        "SafeChip", "unsat",
    )])));

    let cdg = ConstraintDependencyGraph {
        chips: vec![ChipNode {
            name: "SafeChip".to_string(),
            crate_name: "zk".to_string(),
            configure_span: Some(MethodSpan {
                file: PathBuf::from("src/safe_chip.rs"),
                line_start: 10,
                line_end: 20,
            }),
            synthesize_span: None,
            columns: vec!["advice_safe".to_string()],
        }],
        edges: vec![],
        risk_annotations: vec![RiskAnnotation::IsolatedNode {
            chip: "SafeChip".to_string(),
            column: "advice_safe".to_string(),
        }],
    };

    let findings = checker.check_high_risk_nodes(&cdg, &budget()).await;
    assert!(findings.is_empty());
}
