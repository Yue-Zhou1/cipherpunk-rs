use std::fs;
use std::path::PathBuf;

use audit_agent_core::audit_config::{
    AuditConfig, BudgetConfig, EngineConfig, LlmConfig, OptionalInputs, ResolvedScope,
    ResolvedSource, SourceOrigin,
};
use audit_agent_core::finding::Framework;
use audit_agent_core::workspace::{CrateKind, CrateMeta};
use intake::config::ConfigParser;
use intake::confirmation::{CrateDecision, IntakeWarning};
use tauri_ui::{
    OutputType, branch_resolution_banner, crate_decision_style, download_output, export_audit_yaml,
    get_reproduce_preview, llm_missing_details, warning_message,
};
use tempfile::tempdir;

fn sample_config() -> AuditConfig {
    AuditConfig {
        audit_id: "audit-test".to_string(),
        source: ResolvedSource {
            local_path: PathBuf::from("/tmp/repo"),
            origin: SourceOrigin::Local {
                original_path: PathBuf::from("/tmp/repo"),
            },
            commit_hash: "abcdef1234567890abcdef1234567890abcdef12".to_string(),
            content_hash: "sha256:content".to_string(),
        },
        scope: ResolvedScope {
            target_crates: vec!["rollup-core".to_string()],
            excluded_crates: vec!["bench".to_string()],
            build_matrix: vec![],
            detected_frameworks: vec![Framework::SP1],
        },
        engines: EngineConfig {
            crypto_zk: true,
            distributed: false,
        },
        budget: BudgetConfig {
            kani_timeout_secs: 300,
            z3_timeout_secs: 600,
            fuzz_duration_secs: 3600,
            madsim_ticks: 100_000,
            max_llm_retries: 3,
            semantic_index_timeout_secs: 120,
        },
        optional_inputs: OptionalInputs {
            spec_document: None,
            previous_audit: None,
            custom_invariants: vec![],
            known_entry_points: vec![],
        },
        llm: LlmConfig {
            api_key_present: false,
            provider: None,
            no_llm_prose: false,
        },
        output_dir: PathBuf::from("audit-output"),
    }
}

#[test]
fn branch_resolution_banner_uses_pinned_sha_message() {
    let warnings = vec![IntakeWarning::BranchResolved {
        branch: "main".to_string(),
        resolved_sha: "abc123def456".to_string(),
    }];
    let banner = branch_resolution_banner(&warnings).expect("banner");
    assert_eq!(
        banner,
        "Resolved to SHA abc123 — audit is pinned to this commit"
    );
}

#[test]
fn crate_decision_styles_cover_all_variants() {
    let meta = CrateMeta {
        name: "rollup-core".to_string(),
        path: PathBuf::from("/tmp/repo/rollup-core"),
        kind: CrateKind::Lib,
        dependencies: vec![],
    };

    assert_eq!(
        crate_decision_style(&CrateDecision::InScope { meta: meta.clone() }),
        tauri_ui::CrateDecisionStyle::InScope
    );
    assert_eq!(
        crate_decision_style(&CrateDecision::Excluded {
            meta: meta.clone(),
            reason: "bench".to_string(),
        }),
        tauri_ui::CrateDecisionStyle::Excluded
    );
    assert_eq!(
        crate_decision_style(&CrateDecision::Ambiguous {
            meta,
            suggestion: "review".to_string(),
        }),
        tauri_ui::CrateDecisionStyle::Ambiguous
    );
}

#[test]
fn llm_warning_exposes_degraded_feature_list() {
    let warnings = vec![IntakeWarning::LlmKeyMissing {
        degraded_features: vec![
            "Spec normalization".to_string(),
            "Prose rendering".to_string(),
        ],
    }];
    let details = llm_missing_details(&warnings).expect("llm details");
    assert_eq!(details.len(), 2);
    let message = warning_message(&warnings[0]);
    assert!(message.contains("Prose rendering"));
}

#[test]
fn export_audit_yaml_roundtrips_with_config_parser() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("audit.yaml");
    export_audit_yaml(&sample_config(), &path).expect("export yaml");
    let parsed = ConfigParser::parse(&path);
    assert!(parsed.is_ok(), "exported yaml should parse in intake");
}

#[test]
fn download_output_supports_all_six_phase5_outputs() {
    let dir = tempdir().expect("tempdir");
    let output_dir = dir.path().join("audit-output");
    fs::create_dir_all(output_dir.join("regression-tests")).expect("mkdir");
    fs::write(output_dir.join("report-executive.pdf"), "exec-pdf").expect("write");
    fs::write(output_dir.join("report-technical.pdf"), "tech-pdf").expect("write");
    fs::write(output_dir.join("evidence-pack.zip"), "evidence").expect("write");
    fs::write(output_dir.join("findings.sarif"), "{}").expect("write");
    fs::write(output_dir.join("findings.json"), "[]").expect("write");
    fs::write(
        output_dir.join("regression-tests/crypto_misuse_tests.rs"),
        "#[test] fn x() {}",
    )
    .expect("write");

    let variants = [
        OutputType::ExecutivePdf,
        OutputType::TechnicalPdf,
        OutputType::EvidencePackZip,
        OutputType::FindingsSarif,
        OutputType::FindingsJson,
        OutputType::RegressionTestsZip,
    ];
    for (idx, variant) in variants.into_iter().enumerate() {
        let dest = dir.path().join(format!("download-{idx}.bin"));
        download_output(&output_dir, variant, &dest).expect("download output");
        assert!(dest.exists(), "downloaded file should exist");
    }
}

#[test]
fn reproduce_preview_returns_inline_copyable_script() {
    let dir = tempdir().expect("tempdir");
    let evidence_root = dir.path().join("evidence-pack");
    fs::create_dir_all(evidence_root.join("F-TEST-1")).expect("mkdir");
    fs::write(
        evidence_root.join("F-TEST-1/reproduce.sh"),
        "#!/usr/bin/env bash\necho ok\n",
    )
    .expect("write script");

    let preview = get_reproduce_preview(&evidence_root, "F-TEST-1").expect("preview");
    assert!(preview.copyable);
    assert!(preview.script.contains("echo ok"));
}
