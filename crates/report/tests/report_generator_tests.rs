use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use audit_agent_core::audit_config::{
    OptionalInputsSummary, ParsedPreviousAudit, PriorFinding, PriorFindingStatus, ResolvedScope,
    ResolvedSource, SourceOrigin,
};
use audit_agent_core::finding::{
    CodeLocation, Evidence, Finding, FindingCategory, FindingId, FindingStatus, Framework,
    Severity, VerificationStatus,
};
use audit_agent_core::output::{AuditManifest, FindingCounts};
use chrono::Utc;
use llm::{CompletionOpts, LlmProvider};
use report::generator::{ReportGenerator, ReportGeneratorOptions};
use tempfile::tempdir;

fn sample_manifest() -> AuditManifest {
    AuditManifest {
        audit_id: "audit-20260305-abcdef12".to_string(),
        agent_version: "0.1.0".to_string(),
        source: ResolvedSource {
            local_path: PathBuf::from("/tmp/repo"),
            origin: SourceOrigin::Local {
                original_path: PathBuf::from("/tmp/repo"),
            },
            commit_hash: "abcdef1234567890abcdef1234567890abcdef12".to_string(),
            content_hash: "sha256:content".to_string(),
        },
        started_at: Utc::now(),
        completed_at: None,
        scope: ResolvedScope {
            target_crates: vec!["rollup-core".to_string()],
            excluded_crates: vec![],
            build_matrix: vec![],
            detected_frameworks: vec![Framework::SP1],
        },
        tool_versions: HashMap::new(),
        container_digests: HashMap::new(),
        finding_counts: FindingCounts::default(),
        risk_score: 55,
        engines_run: vec!["crypto_zk".to_string()],
        optional_inputs_used: OptionalInputsSummary {
            spec_provided: true,
            prev_audit_provided: false,
            invariants_count: 0,
            entry_points_count: 0,
            llm_prose_used: false,
        },
    }
}

fn sample_finding() -> Finding {
    Finding {
        id: FindingId::new("F-CRYPTO-0100"),
        title: "Synthetic report generator finding".to_string(),
        severity: Severity::High,
        category: FindingCategory::CryptoMisuse,
        framework: Framework::SP1,
        affected_components: vec![CodeLocation {
            crate_name: "rollup-core".to_string(),
            module: "lib".to_string(),
            file: PathBuf::from("rollup-core/src/lib.rs"),
            line_range: (10, 14),
            snippet: Some("unsafe_hash(bytes);".to_string()),
        }],
        prerequisites: "attacker can control bytes".to_string(),
        exploit_path: "crafted bytes collide".to_string(),
        impact: "integrity risk".to_string(),
        evidence: Evidence {
            command: Some("bash evidence-pack/F-CRYPTO-0100/reproduce.sh".to_string()),
            seed: None,
            trace_file: None,
            counterexample: None,
            harness_path: None,
            smt2_file: None,
            container_digest: "sha256:deadbeef".to_string(),
            tool_versions: HashMap::new(),
        },
        evidence_gate_level: 0,
        llm_generated: false,
        recommendation: "replace unsafe hash primitive".to_string(),
        regression_test: Some("test_safe_hash_required".to_string()),
        status: FindingStatus::Open,
        regression_check: false,
        verification_status: VerificationStatus::Verified,
    }
}

#[derive(Debug)]
struct CountingProvider {
    calls: Arc<AtomicUsize>,
}

#[async_trait]
impl LlmProvider for CountingProvider {
    async fn complete(&self, _prompt: &str, _opts: &CompletionOpts) -> anyhow::Result<String> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok("polished text".to_string())
    }

    fn name(&self) -> &str {
        "counting"
    }

    fn is_available(&self) -> bool {
        true
    }
}

#[tokio::test]
async fn generate_all_without_llm_writes_outputs_and_marks_manifest_false() {
    let dir = tempdir().expect("tempdir");
    let evidence_zip = dir.path().join("evidence-pack.zip");
    std::fs::write(&evidence_zip, "zip-bytes").expect("write evidence zip");

    let generator = ReportGenerator::new(
        vec![sample_finding()],
        sample_manifest(),
        ReportGeneratorOptions {
            llm: None,
            no_llm_prose: false,
            evidence_pack_zip: evidence_zip,
            previous_audit: None,
        },
    );

    generator
        .generate_all(dir.path())
        .await
        .expect("generate outputs");

    for relative in [
        "report-executive.md",
        "report-technical.md",
        "report-executive.pdf",
        "report-technical.pdf",
        "findings.json",
        "findings.sarif",
        "audit-manifest.json",
        "evidence-pack.zip",
        "regression-tests/crypto_misuse_tests.rs",
    ] {
        assert!(
            dir.path().join(relative).exists(),
            "missing output file {relative}"
        );
    }

    let manifest_text =
        std::fs::read_to_string(dir.path().join("audit-manifest.json")).expect("read manifest");
    let manifest: AuditManifest = serde_json::from_str(&manifest_text).expect("parse manifest");
    assert!(!manifest.optional_inputs_used.llm_prose_used);
}

#[tokio::test]
async fn no_llm_prose_flag_disables_provider_calls() {
    let dir = tempdir().expect("tempdir");
    let evidence_zip = dir.path().join("evidence-pack.zip");
    std::fs::write(&evidence_zip, "zip-bytes").expect("write evidence zip");
    let calls = Arc::new(AtomicUsize::new(0));
    let llm = Arc::new(CountingProvider {
        calls: Arc::clone(&calls),
    });

    let generator = ReportGenerator::new(
        vec![sample_finding()],
        sample_manifest(),
        ReportGeneratorOptions {
            llm: Some(llm),
            no_llm_prose: true,
            evidence_pack_zip: evidence_zip,
            previous_audit: None,
        },
    );

    generator
        .generate_all(dir.path())
        .await
        .expect("generate outputs");
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn llm_prose_enabled_calls_provider_and_sets_manifest_true() {
    let dir = tempdir().expect("tempdir");
    let evidence_zip = dir.path().join("evidence-pack.zip");
    std::fs::write(&evidence_zip, "zip-bytes").expect("write evidence zip");
    let calls = Arc::new(AtomicUsize::new(0));
    let llm = Arc::new(CountingProvider {
        calls: Arc::clone(&calls),
    });

    let generator = ReportGenerator::new(
        vec![sample_finding()],
        sample_manifest(),
        ReportGeneratorOptions {
            llm: Some(llm),
            no_llm_prose: false,
            evidence_pack_zip: evidence_zip,
            previous_audit: None,
        },
    );

    generator
        .generate_all(dir.path())
        .await
        .expect("generate outputs");

    assert!(calls.load(Ordering::SeqCst) > 0);

    let report_text = std::fs::read_to_string(dir.path().join("report-technical.md"))
        .expect("read technical report");
    assert!(
        report_text.contains("polished text"),
        "report should contain polished text when llm prose is enabled"
    );

    let manifest_text =
        std::fs::read_to_string(dir.path().join("audit-manifest.json")).expect("read manifest");
    let manifest: AuditManifest = serde_json::from_str(&manifest_text).expect("parse manifest");
    assert!(manifest.optional_inputs_used.llm_prose_used);
}

#[tokio::test]
async fn previous_audit_matching_finding_is_marked_as_regression_check() {
    let dir = tempdir().expect("tempdir");
    let evidence_zip = dir.path().join("evidence-pack.zip");
    std::fs::write(&evidence_zip, "zip-bytes").expect("write evidence zip");

    let previous_audit = ParsedPreviousAudit {
        source_path: PathBuf::from("/tmp/previous-audit.md"),
        prior_findings: vec![PriorFinding {
            id: "F-CRYPTO-0100".to_string(),
            title: "Synthetic report generator finding".to_string(),
            severity: Severity::High,
            description: "old finding".to_string(),
            status: PriorFindingStatus::Reported,
            location_hint: None,
        }],
    };

    let generator = ReportGenerator::new(
        vec![sample_finding()],
        sample_manifest(),
        ReportGeneratorOptions {
            llm: None,
            no_llm_prose: false,
            evidence_pack_zip: evidence_zip,
            previous_audit: Some(previous_audit),
        },
    );

    generator
        .generate_all(dir.path())
        .await
        .expect("generate outputs");

    let findings_text =
        std::fs::read_to_string(dir.path().join("findings.json")).expect("read findings");
    let findings: Vec<Finding> = serde_json::from_str(&findings_text).expect("parse findings");
    assert!(findings[0].regression_check);
}

#[tokio::test]
async fn executive_pdf_stub_is_capped_to_two_page_budget() {
    let dir = tempdir().expect("tempdir");
    let evidence_zip = dir.path().join("evidence-pack.zip");
    std::fs::write(&evidence_zip, "zip-bytes").expect("write evidence zip");

    let findings = (0..200)
        .map(|idx| {
            let mut finding = sample_finding();
            finding.id = FindingId::new(format!("F-CRYPTO-{idx:04}"));
            finding.title = format!("Synthetic finding {idx}");
            finding
        })
        .collect::<Vec<_>>();

    let generator = ReportGenerator::new(
        findings,
        sample_manifest(),
        ReportGeneratorOptions {
            llm: None,
            no_llm_prose: false,
            evidence_pack_zip: evidence_zip,
            previous_audit: None,
        },
    );
    generator
        .generate_all(dir.path())
        .await
        .expect("generate outputs");

    let pdf_stub = std::fs::read_to_string(dir.path().join("report-executive.pdf"))
        .expect("read executive pdf stub");
    assert!(
        pdf_stub.lines().count() <= 120,
        "executive pdf stub exceeded 2-page budget"
    );
}

#[tokio::test]
async fn generated_regression_tests_build_with_cargo_in_temp_workspace() {
    let dir = tempdir().expect("tempdir");
    let evidence_zip = dir.path().join("evidence-pack.zip");
    std::fs::write(&evidence_zip, "zip-bytes").expect("write evidence zip");

    let generator = ReportGenerator::new(
        vec![sample_finding()],
        sample_manifest(),
        ReportGeneratorOptions {
            llm: None,
            no_llm_prose: false,
            evidence_pack_zip: evidence_zip,
            previous_audit: None,
        },
    );
    generator
        .generate_all(dir.path())
        .await
        .expect("generate outputs");

    let workspace = tempdir().expect("workspace");
    let crate_dir = workspace.path().join("audit-regression");
    std::fs::create_dir_all(crate_dir.join("src")).expect("mkdir");
    std::fs::write(
        workspace.path().join("Cargo.toml"),
        "[workspace]\nmembers = [\"audit-regression\"]\nresolver = \"2\"\n",
    )
    .expect("write workspace manifest");
    std::fs::write(
        crate_dir.join("Cargo.toml"),
        "[package]\nname = \"audit-regression\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("write crate manifest");
    std::fs::copy(
        dir.path().join("regression-tests/crypto_misuse_tests.rs"),
        crate_dir.join("src/lib.rs"),
    )
    .expect("copy generated regression file");

    let output = Command::new("cargo")
        .arg("build")
        .current_dir(workspace.path())
        .output()
        .expect("run cargo build");
    assert!(
        output.status.success(),
        "generated regression tests should build with cargo:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
