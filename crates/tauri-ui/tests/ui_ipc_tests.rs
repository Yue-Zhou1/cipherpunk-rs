use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use audit_agent_core::audit_config::{
    AuditConfig, BudgetConfig, EngineConfig, LlmConfig, OptionalInputs, ResolvedScope,
    ResolvedSource, SourceOrigin,
};
use audit_agent_core::finding::Framework;
use audit_agent_core::workspace::{CrateKind, CrateMeta};
use intake::config::ConfigParser;
use intake::confirmation::{CrateDecision, IntakeWarning};
use tauri_ui::ipc::{
    ConfirmWorkspaceRequest, SourceInputIpc, SourceKind, ToolbenchSelectionRequest, UiSessionState,
};
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

fn run_git(dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("run git command");
    if !output.status.success() {
        panic!(
            "git {:?} failed:\nstdout:\n{}\nstderr:\n{}",
            args,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

fn create_local_workspace_repo(root: &Path) -> (PathBuf, String) {
    let repo_root = root.join("repo");
    fs::create_dir_all(repo_root.join("rollup-core/src")).expect("create repo crate");
    fs::write(
        repo_root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"rollup-core\"]\nresolver = \"2\"\n",
    )
    .expect("write workspace manifest");
    fs::write(
        repo_root.join("rollup-core/Cargo.toml"),
        "[package]\nname = \"rollup-core\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .expect("write crate manifest");
    fs::write(
        repo_root.join("rollup-core/src/lib.rs"),
        "pub fn verifier_ready() -> bool { true }\n",
    )
    .expect("write crate source");

    run_git(&repo_root, &["init", "-q"]);
    run_git(&repo_root, &["config", "user.name", "test"]);
    run_git(&repo_root, &["config", "user.email", "test@example.com"]);
    run_git(&repo_root, &["add", "."]);
    run_git(&repo_root, &["commit", "-qm", "initial"]);

    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&repo_root)
        .output()
        .expect("read git sha");
    if !output.status.success() {
        panic!(
            "git rev-parse HEAD failed:\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let sha = String::from_utf8_lossy(&output.stdout).trim().to_string();

    (repo_root, sha)
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

    let yaml = fs::read_to_string(&path).expect("read exported yaml");
    assert!(
        yaml.contains("llm:"),
        "exported yaml should include llm section from resolved config"
    );
    assert!(
        yaml.contains("optional_inputs:"),
        "exported yaml should include optional_inputs section from resolved config"
    );
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
fn download_output_uses_markdown_report_when_pdf_is_unavailable() {
    let dir = tempdir().expect("tempdir");
    let output_dir = dir.path().join("audit-output");
    fs::create_dir_all(&output_dir).expect("mkdir");
    fs::write(
        output_dir.join("report-executive.md"),
        "# executive report markdown only",
    )
    .expect("write markdown");

    let dest = dir.path().join("download-exec.md");
    download_output(&output_dir, OutputType::ExecutivePdf, &dest).expect("download output");

    let content = fs::read_to_string(dest).expect("read downloaded report");
    assert!(content.contains("executive report markdown only"));
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

#[test]
fn output_type_serializes_as_snake_case_for_frontend_contract() {
    let parsed: OutputType = serde_json::from_str("\"findings_json\"").expect("deserialize");
    assert_eq!(parsed, OutputType::FindingsJson);

    let encoded = serde_json::to_string(&OutputType::RegressionTestsZip).expect("serialize");
    assert_eq!(encoded, "\"regression_tests_zip\"");
}

#[test]
fn session_confirm_workspace_requires_source_resolution_first() {
    let mut session = UiSessionState::new(PathBuf::from(".audit-work"));
    let error = session
        .confirm_workspace(ConfirmWorkspaceRequest {
            confirmed: true,
            ambiguous_crates: HashMap::new(),
            no_llm_prose: false,
        })
        .expect_err("confirm_workspace should enforce source resolution order");
    assert!(
        error
            .to_string()
            .contains("resolve_source must be called before confirm_workspace")
    );
}

#[tokio::test(flavor = "current_thread")]
async fn session_flow_exports_yaml_and_downloads_output_after_confirmation() {
    let dir = tempdir().expect("tempdir");
    let (repo_root, commit_sha) = create_local_workspace_repo(dir.path());

    let mut session = UiSessionState::new(dir.path().join(".audit-work"));
    let resolved = session
        .resolve_source(SourceInputIpc {
            kind: SourceKind::Local,
            value: repo_root.display().to_string(),
            commit_or_ref: Some(commit_sha.clone()),
        })
        .await
        .expect("resolve local source");
    assert_eq!(resolved.source.commit_hash, commit_sha);

    let summary = session.detect_workspace().expect("detect workspace");
    assert!(
        summary.crates.iter().any(|decision| matches!(
            decision,
            CrateDecision::InScope { meta } if meta.name == "rollup-core"
        )),
        "workspace detection should include rollup-core as in-scope"
    );

    let confirmation = session
        .confirm_workspace(ConfirmWorkspaceRequest {
            confirmed: true,
            ambiguous_crates: HashMap::new(),
            no_llm_prose: false,
        })
        .expect("confirm workspace");
    assert!(confirmation.audit_id.starts_with("audit-"));

    let output_dir = session
        .audit_config()
        .expect("audit config set after confirmation")
        .output_dir
        .clone();
    fs::create_dir_all(&output_dir).expect("mkdir output");
    fs::write(output_dir.join("findings.json"), "[]").expect("write findings");

    let yaml_path = dir.path().join("resolved-audit.yaml");
    session
        .export_audit_yaml(&yaml_path)
        .expect("export resolved yaml");
    assert!(yaml_path.exists(), "resolved audit yaml should be exported");

    let dest = dir.path().join("downloads/findings.json");
    let response = session
        .download_output(&confirmation.audit_id, OutputType::FindingsJson, &dest)
        .expect("download output");
    assert_eq!(response.dest, dest);
    assert_eq!(
        fs::read_to_string(response.dest).expect("read downloaded output"),
        "[]"
    );
}

#[tokio::test]
async fn confirm_workspace_creates_session_id_and_snapshot() {
    let mut session = UiSessionState::new(PathBuf::from(".audit-work"));
    let response = session
        .create_audit_session_for_tests()
        .await
        .expect("create session");
    assert!(response.session_id.starts_with("sess-"));
    assert!(response.snapshot_id.starts_with("snap-"));
}

#[tokio::test(flavor = "current_thread")]
async fn workstation_commands_return_tree_file_and_console_data() {
    let dir = tempdir().expect("tempdir");
    let (repo_root, commit_sha) = create_local_workspace_repo(dir.path());

    let mut session = UiSessionState::new(dir.path().join(".audit-work"));
    session
        .resolve_source(SourceInputIpc {
            kind: SourceKind::Local,
            value: repo_root.display().to_string(),
            commit_or_ref: Some(commit_sha),
        })
        .await
        .expect("resolve local source");
    session.detect_workspace().expect("detect workspace");
    session
        .confirm_workspace(ConfirmWorkspaceRequest {
            confirmed: true,
            ambiguous_crates: HashMap::new(),
            no_llm_prose: false,
        })
        .expect("confirm workspace");

    let created = session
        .create_audit_session()
        .await
        .expect("create audit session");

    let tree = session
        .get_project_tree(&created.session_id)
        .await
        .expect("project tree");
    assert!(!tree.nodes.is_empty(), "project tree should include files");

    let file = session
        .read_source_file(&created.session_id, "rollup-core/src/lib.rs")
        .await
        .expect("read source file");
    assert!(
        file.content.contains("verifier_ready"),
        "expected repo content"
    );

    let console = session
        .tail_session_console(&created.session_id, 20)
        .expect("tail console");
    assert!(
        !console.entries.is_empty(),
        "console should include bootstrap lifecycle events"
    );

    let file_graph = session
        .load_file_graph(&created.session_id)
        .await
        .expect("load file graph");
    assert_eq!(file_graph.lens, "file");

    let feature_graph = session
        .load_feature_graph(&created.session_id)
        .await
        .expect("load feature graph");
    assert_eq!(feature_graph.lens, "feature");

    let dataflow_graph = session
        .load_dataflow_graph(&created.session_id, false)
        .await
        .expect("load redacted dataflow graph");
    assert_eq!(dataflow_graph.lens, "dataflow");
    assert!(dataflow_graph.redacted_values);

    let overview = session
        .load_security_overview(&created.session_id)
        .await
        .expect("load security overview");
    assert!(
        !overview.trust_boundaries.is_empty(),
        "security overview should include trust boundaries"
    );

    let checklist = session
        .load_checklist_plan(&created.session_id)
        .await
        .expect("load checklist plan");
    assert!(
        !checklist.domains.is_empty(),
        "checklist plan should include at least one domain"
    );

    let toolbench = session
        .load_toolbench_context(
            &created.session_id,
            ToolbenchSelectionRequest {
                kind: "symbol".to_string(),
                id: "prove".to_string(),
            },
        )
        .await
        .expect("load toolbench context");
    assert_eq!(toolbench.selection.kind, "symbol");
    assert_eq!(toolbench.selection.id, "prove");
    assert!(
        !toolbench.recommended_tools.is_empty(),
        "toolbench should include recommendations"
    );
    assert_eq!(
        toolbench.domains.len(),
        checklist.domains.len(),
        "toolbench domains should mirror checklist domains"
    );
}
