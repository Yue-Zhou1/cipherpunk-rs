use std::fs;
use std::path::PathBuf;

use engine_crypto::zk::circom::signal_graph::{CircomSignalGraph, SignalKind};
use num_bigint::BigUint;
use tempfile::tempdir;

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/circom")
        .join(name)
}

#[test]
fn parses_circomlib_comparators_fixture() {
    let graph = CircomSignalGraph::from_file(&fixture_path("comparators.circom"))
        .expect("parse comparators.circom fixture");

    assert!(graph.templates.contains_key("LessThan"));
    assert!(graph.templates.contains_key("IsZero"));
    assert!(
        graph
            .signals
            .iter()
            .any(|s| s.template == "LessThan" && s.name == "out" && s.kind == SignalKind::Output)
    );
    assert!(
        !graph.constraints.is_empty(),
        "expected at least one parsed Circom constraint"
    );
}

#[test]
fn finds_trivially_unconstrained_outputs() {
    let dir = tempdir().expect("tempdir");
    let circuit = dir.path().join("unconstrained.circom");
    fs::write(
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

    let graph = CircomSignalGraph::from_file(&circuit).expect("parse fixture");
    let unconstrained = graph.find_trivially_unconstrained();
    assert_eq!(unconstrained.len(), 1);
    assert_eq!(unconstrained[0].name, "leaked");
    assert_eq!(unconstrained[0].template, "Leak");
}

#[test]
fn exports_smt2_for_target_signal() {
    let graph = CircomSignalGraph::from_file(&fixture_path("comparators.circom"))
        .expect("parse comparators.circom fixture");
    let field_prime = BigUint::parse_bytes(
        b"21888242871839275222246405745257275088548364400416034343698204186575808495617",
        10,
    )
    .expect("prime");

    let smt2 = graph.to_smt2("out", &field_prime);

    assert!(smt2.contains("(set-logic QF_NIA)"));
    assert!(smt2.contains("(check-sat)"));
    assert!(smt2.contains("(get-model)"));
    assert!(
        smt2.contains("target outputs differ"),
        "SMT2 should enforce different outputs across two witnesses"
    );
}

#[test]
fn exported_smt2_parses_with_real_smt2_parser() {
    let graph = CircomSignalGraph::from_file(&fixture_path("comparators.circom"))
        .expect("parse comparators.circom fixture");
    let field_prime = BigUint::parse_bytes(
        b"21888242871839275222246405745257275088548364400416034343698204186575808495617",
        10,
    )
    .expect("prime");
    let smt2 = graph.to_smt2("out", &field_prime);

    let stream = smt2parser::CommandStream::new(
        std::io::Cursor::new(smt2.as_bytes()),
        smt2parser::concrete::SyntaxBuilder::default(),
        None,
    );
    for command in stream {
        command.expect("SMT2 command should parse");
    }
}

#[test]
fn preserves_arithmetic_structure_in_smt_translation() {
    let dir = tempdir().expect("tempdir");
    let circuit = dir.path().join("arith.circom");
    fs::write(
        &circuit,
        r#"
template Arith() {
    signal input in;
    signal input inv;
    signal output out;
    out <== -in*inv + 1;
}
"#,
    )
    .expect("write fixture");
    let graph = CircomSignalGraph::from_file(&circuit).expect("parse fixture");
    let prime = BigUint::parse_bytes(b"23", 10).expect("prime");
    let smt2 = graph.to_smt2("out", &prime);

    assert!(
        smt2.contains("(*"),
        "non-trivial multiplication should survive SMT translation"
    );
    assert!(
        smt2.contains(" 1") || smt2.contains(" 1)"),
        "constant terms should survive SMT translation"
    );
}

#[test]
fn malformed_circom_is_rejected() {
    let dir = tempdir().expect("tempdir");
    let circuit = dir.path().join("bad.circom");
    fs::write(
        &circuit,
        r#"
template Broken() {
    signal input in;
    signal output out;
    out <== in;
"#,
    )
    .expect("write fixture");

    let err = CircomSignalGraph::from_file(&circuit).expect_err("malformed circom must fail");
    assert!(
        err.to_string().to_ascii_lowercase().contains("brace"),
        "error should explain malformed structure"
    );
}
