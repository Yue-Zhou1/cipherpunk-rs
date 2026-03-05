use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use audit_agent_core::audit_config::BudgetConfig;
use engine_crypto::zk::circom::signal_graph::CircomSignalGraph;
use engine_crypto::zk::circom::z3_checker::{
    CounterexamplePair, Z3CheckResult, Z3ExecutionOutput, Z3ExecutionRunner,
    Z3UnderConstrainedChecker,
};
use tempfile::tempdir;

#[derive(Clone)]
struct StubRunner {
    output: Z3ExecutionOutput,
}

#[async_trait]
impl Z3ExecutionRunner for StubRunner {
    async fn execute(&self, _smt2_file: &Path, _timeout_secs: u64) -> Result<Z3ExecutionOutput> {
        Ok(self.output.clone())
    }
}

#[derive(Clone)]
struct LessThanSatRunner;

#[async_trait]
impl Z3ExecutionRunner for LessThanSatRunner {
    async fn execute(&self, smt2_file: &Path, _timeout_secs: u64) -> Result<Z3ExecutionOutput> {
        let smt2 = fs::read_to_string(smt2_file).expect("read smt2 query");
        let symbol_base = smt2
            .lines()
            .find_map(|line| {
                let trimmed = line.trim();
                if !trimmed.starts_with("; witness-symbol ") {
                    return None;
                }
                let rest = trimmed.trim_start_matches("; witness-symbol ");
                let mut parts = rest.splitn(2, " -> ");
                let symbol = parts.next()?.trim();
                let logical = parts.next()?.trim();
                if logical == "LessThan::out" {
                    Some(symbol.to_string())
                } else {
                    None
                }
            })
            .expect("LessThan::out witness symbol");

        Ok(Z3ExecutionOutput {
            stdout: format!(
                "sat\n(model\n  (define-fun {base}__a () Int 0)\n  (define-fun {base}__b () Int 1)\n)\n",
                base = symbol_base
            ),
            stderr: String::new(),
            exit_code: 0,
            container_digest: "sha256:z3-less-than".to_string(),
        })
    }
}

fn budget() -> BudgetConfig {
    BudgetConfig {
        kani_timeout_secs: 300,
        z3_timeout_secs: 30,
        fuzz_duration_secs: 60,
        madsim_ticks: 1_000,
        max_llm_retries: 1,
    }
}

#[tokio::test]
async fn returns_under_constrained_when_z3_reports_sat() {
    let runner = Arc::new(StubRunner {
        output: Z3ExecutionOutput {
            stdout: r#"
sat
(model
  (define-fun t_Leak_in_0__a () Int 1)
  (define-fun t_Leak_in_0__b () Int 1)
  (define-fun t_Leak_out_1__a () Int 2)
  (define-fun t_Leak_out_1__b () Int 3)
)
"#
            .trim()
            .to_string(),
            stderr: String::new(),
            exit_code: 0,
            container_digest: "sha256:z3-image".to_string(),
        },
    });
    let checker = Z3UnderConstrainedChecker::new(runner);
    let result = checker
        .check("(set-logic QF_NIA)\n(check-sat)\n(get-model)\n", &budget())
        .await
        .expect("check result");

    match result {
        Z3CheckResult::UnderConstrained {
            witness_a,
            witness_b,
            container_digest,
            ..
        } => {
            assert_eq!(container_digest, "sha256:z3-image");
            assert_eq!(witness_a.get("Leak::out"), Some(&2u32.into()));
            assert_eq!(witness_b.get("Leak::out"), Some(&3u32.into()));
        }
        other => panic!("expected UnderConstrained, got {other:?}"),
    }
}

#[tokio::test]
async fn unknown_result_falls_back_to_random_search_with_seed() {
    let runner = Arc::new(StubRunner {
        output: Z3ExecutionOutput {
            stdout: "unknown\n".to_string(),
            stderr: "timeout".to_string(),
            exit_code: 0,
            container_digest: "sha256:z3-image".to_string(),
        },
    });
    let checker = Z3UnderConstrainedChecker::new(runner);

    let dir = tempdir().expect("tempdir");
    let circuit = dir.path().join("fallback.circom");
    std::fs::write(
        &circuit,
        r#"
template Leak() {
    signal input in;
    signal output out;
    signal output leaked;
    out <== in;
    leaked <-- in;
}
"#,
    )
    .expect("write fixture");
    let graph = CircomSignalGraph::from_file(&circuit).expect("parse graph");

    let result = checker
        .check_with_graph(
            "(set-logic QF_NIA)\n(check-sat)\n(get-model)\n",
            &budget(),
            &graph,
            1337,
        )
        .await
        .expect("check result");

    match result {
        Z3CheckResult::Unknown {
            fallback_result,
            seed,
            container_digest,
            ..
        } => {
            assert_eq!(container_digest, "sha256:z3-image");
            assert_eq!(seed, Some(1337));
            assert!(matches!(
                fallback_result,
                Some(CounterexamplePair {
                    witness_a: _,
                    witness_b: _
                })
            ));
        }
        other => panic!("expected Unknown variant, got {other:?}"),
    }
}

#[tokio::test]
async fn random_search_does_not_report_counterexample_when_constraints_unsat() {
    let runner = Arc::new(StubRunner {
        output: Z3ExecutionOutput {
            stdout: "unknown\n".to_string(),
            stderr: "timeout".to_string(),
            exit_code: 0,
            container_digest: "sha256:z3-image".to_string(),
        },
    });
    let checker = Z3UnderConstrainedChecker::new(runner);

    let dir = tempdir().expect("tempdir");
    let circuit = dir.path().join("unsat.circom");
    std::fs::write(
        &circuit,
        r#"
template Impossible() {
    signal output leaked;
    1 === 0;
}
"#,
    )
    .expect("write fixture");
    let graph = CircomSignalGraph::from_file(&circuit).expect("parse graph");

    let result = checker
        .check_with_graph(
            "(set-logic QF_NIA)\n(check-sat)\n(get-model)\n",
            &budget(),
            &graph,
            99,
        )
        .await
        .expect("check result");

    match result {
        Z3CheckResult::Unknown {
            fallback_result, ..
        } => {
            assert!(
                fallback_result.is_none(),
                "fallback must not fabricate counterexamples for unsat constraints"
            );
        }
        other => panic!("expected Unknown variant, got {other:?}"),
    }
}

#[tokio::test]
async fn unsat_result_marks_graph_as_constrained() {
    let runner = Arc::new(StubRunner {
        output: Z3ExecutionOutput {
            stdout: "unsat\n".to_string(),
            stderr: String::new(),
            exit_code: 0,
            container_digest: "sha256:z3-image".to_string(),
        },
    });
    let checker = Z3UnderConstrainedChecker::new(runner);

    let result = checker
        .check("(set-logic QF_NIA)\n(check-sat)\n", &budget())
        .await
        .expect("check result");

    match result {
        Z3CheckResult::Constrained {
            container_digest, ..
        } => {
            assert_eq!(container_digest, "sha256:z3-image");
        }
        other => panic!("expected Constrained variant, got {other:?}"),
    }
}

#[tokio::test]
async fn detects_under_constrained_less_than_gadget_from_circom_fixture() {
    let graph = CircomSignalGraph::from_file(
        &std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/circom/comparators.circom"),
    )
    .expect("parse comparators fixture");
    let prime = num_bigint::BigUint::parse_bytes(
        b"21888242871839275222246405745257275088548364400416034343698204186575808495617",
        10,
    )
    .expect("prime");
    let smt2 = graph.to_smt2("out", &prime);

    let checker = Z3UnderConstrainedChecker::new(Arc::new(LessThanSatRunner));
    let result = checker
        .check_with_graph(&smt2, &budget(), &graph, 7)
        .await
        .expect("z3 check");

    match result {
        Z3CheckResult::UnderConstrained {
            witness_a,
            witness_b,
            container_digest,
            ..
        } => {
            assert_eq!(container_digest, "sha256:z3-less-than");
            assert_eq!(witness_a.get("LessThan::out"), Some(&0u32.into()));
            assert_eq!(witness_b.get("LessThan::out"), Some(&1u32.into()));
        }
        other => panic!("expected UnderConstrained for LessThan gadget, got {other:?}"),
    }
}

#[test]
fn model_parser_extracts_dual_witnesses() {
    let model = r#"
(model
  (define-fun t_Leak_in_0__a () Int 9)
  (define-fun t_Leak_in_0__b () Int 9)
  (define-fun t_Leak_out_1__a () Int 1)
  (define-fun t_Leak_out_1__b () Int 2)
)
"#;
    let (a, b) = engine_crypto::zk::circom::z3_checker::extract_witnesses(
        model,
        &HashMap::from([
            ("t_Leak_in_0".to_string(), "Leak::in".to_string()),
            ("t_Leak_out_1".to_string(), "Leak::out".to_string()),
        ]),
    );

    assert_eq!(a.get("Leak::in"), Some(&9u32.into()));
    assert_eq!(b.get("Leak::in"), Some(&9u32.into()));
    assert_eq!(a.get("Leak::out"), Some(&1u32.into()));
    assert_eq!(b.get("Leak::out"), Some(&2u32.into()));
}
