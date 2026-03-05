use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use audit_agent_core::finding::{
    CodeLocation, Evidence, Finding, FindingCategory, FindingId, FindingStatus, Framework,
    Severity, VerificationStatus,
};
use report::regression::{generate_regression_tests, write_phase1_output_layout};
use tempfile::tempdir;

fn sample_finding() -> Finding {
    Finding {
        id: FindingId::new("F-CRYPTO-0099"),
        title: "Synthetic phase1 finding".to_string(),
        severity: Severity::Medium,
        category: FindingCategory::CryptoMisuse,
        framework: Framework::SP1,
        affected_components: vec![CodeLocation {
            crate_name: "crypto-app".to_string(),
            module: "lib".to_string(),
            file: PathBuf::from("crypto-app/src/lib.rs"),
            line_range: (1, 1),
            snippet: Some("hardcoded_key_material(key);".to_string()),
        }],
        prerequisites: "attacker can call code path".to_string(),
        exploit_path: "trigger rule pattern".to_string(),
        impact: "security risk".to_string(),
        evidence: Evidence {
            command: Some("bash evidence-pack/F-CRYPTO-0099/reproduce.sh".to_string()),
            seed: None,
            trace_file: None,
            counterexample: None,
            harness_path: None,
            smt2_file: None,
            container_digest: "sha256:beef".to_string(),
            tool_versions: HashMap::new(),
        },
        evidence_gate_level: 0,
        llm_generated: false,
        recommendation: "rotate secrets".to_string(),
        regression_test: Some(
            "#[cfg(kani)]\n#[kani::proof]\nfn f_crypto_0099_regression() { kani::assert!(true); }\n"
                .to_string(),
        ),
        status: FindingStatus::Open,
        regression_check: false,
        verification_status: VerificationStatus::Verified,
    }
}

#[test]
fn generated_regression_file_compiles_as_valid_rust() {
    let suite = generate_regression_tests(&[sample_finding()]);
    let source = suite
        .crypto_tests
        .as_ref()
        .expect("expected generated tests")
        .clone();
    let dir = tempdir().expect("tempdir");
    let file = dir.path().join("crypto_misuse_tests.rs");
    fs::write(&file, source).expect("write generated tests");

    let output = Command::new("rustc")
        .arg("--edition=2024")
        .arg("--crate-type")
        .arg("lib")
        .arg(&file)
        .arg("-o")
        .arg(dir.path().join("crypto_misuse_tests.rlib"))
        .output()
        .expect("run rustc");
    assert!(
        output.status.success(),
        "generated tests should compile: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(suite.kani_harnesses.len(), 1);
    assert_eq!(suite.kani_harnesses[0].finding_id, "F-CRYPTO-0099");
}

#[test]
fn phase1_output_layout_matches_io_contract_structure() {
    let dir = tempdir().expect("tempdir");
    let out_dir = dir.path().join("audit-output");
    let evidence_zip = dir.path().join("evidence-pack.zip");
    fs::write(&evidence_zip, "zip-bytes").expect("write dummy evidence zip");

    let suite = generate_regression_tests(&[sample_finding()]);
    write_phase1_output_layout(
        &out_dir,
        "# Executive Summary\n",
        "# Technical\n",
        "[]",
        "{\"version\":\"2.1.0\",\"runs\":[]}",
        &evidence_zip,
        "{\"audit_id\":\"audit\"}",
        &suite,
    )
    .expect("write output layout");

    for relative in [
        "report-executive.md",
        "report-technical.md",
        "findings.json",
        "findings.sarif",
        "evidence-pack.zip",
        "audit-manifest.json",
        "regression-tests/crypto_misuse_tests.rs",
        "regression-tests/kani_harnesses/F-CRYPTO-0099.rs",
    ] {
        assert!(
            out_dir.join(relative).exists(),
            "missing output file {relative}"
        );
    }
}
