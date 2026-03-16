use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use audit_agent_core::audit_config::{
    AuditConfig, BudgetConfig, CustomAssertionTarget, EngineConfig, LlmConfig, OptionalInputs,
    ResolvedScope, ResolvedSource, SourceOrigin, StructuredConstraint,
};
use audit_agent_core::finding::Framework;
use intake::config::{
    ConfigError, ConfigParser, RawAuditConfig, RawBudgetConfig, RawEngineConfig, RawScope,
    RawSource,
};
use intake::confirmation::{ConfirmationSummary, IntakeWarning, WorkspaceConfirmation};
use intake::detection::FrameworkDetector;
use intake::optional_inputs::OptionalInputParser;
use intake::source::{SourceInput, SourceResolver, SourceWarning};
use intake::summarize_optional_inputs;
use intake::workspace::WorkspaceAnalyzer;
use tempfile::TempDir;

fn run(dir: &PathBuf, cmd: &str, args: &[&str]) {
    let output = Command::new(cmd)
        .args(args)
        .current_dir(dir)
        .output()
        .expect("run command");
    if !output.status.success() {
        panic!(
            "{} {:?} failed:\nstdout:\n{}\nstderr:\n{}",
            cmd,
            args,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

fn create_git_workspace() -> (TempDir, String) {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path().to_path_buf();

    fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"member\"]\nresolver = \"2\"\n",
    )
    .expect("write workspace manifest");
    fs::create_dir_all(root.join("member/src")).expect("create src");
    fs::write(
        root.join("member/Cargo.toml"),
        "[package]\nname = \"member\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .expect("write crate manifest");
    fs::write(root.join("member/src/lib.rs"), "pub fn x() -> u8 { 1 }\n").expect("write code");

    run(&root, "git", &["init", "-q"]);
    run(&root, "git", &["config", "user.name", "test"]);
    run(&root, "git", &["config", "user.email", "test@example.com"]);
    run(&root, "git", &["add", "."]);
    run(&root, "git", &["commit", "-qm", "init"]);

    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&root)
        .output()
        .expect("rev-parse");
    let sha = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (temp, sha)
}

#[tokio::test]
async fn source_resolver_clones_specific_commit() {
    let (repo, sha) = create_git_workspace();
    let work = tempfile::tempdir().expect("workdir");
    let url = format!("file://{}", repo.path().display());

    let result = SourceResolver::resolve(
        &SourceInput::GitUrl {
            url,
            commit: sha.clone(),
            auth: None,
            allow_branch_resolution: false,
        },
        work.path(),
    )
    .await
    .expect("resolve source");

    assert_eq!(result.source.commit_hash, sha);
    assert!(result.source.local_path.exists());
    assert!(!result.source.content_hash.is_empty());
    assert!(result.warnings.is_empty());
}

#[tokio::test]
async fn source_resolver_branch_name_requires_opt_in() {
    let (repo, _sha) = create_git_workspace();
    let work = tempfile::tempdir().expect("workdir");
    let url = format!("file://{}", repo.path().display());

    let err = SourceResolver::resolve(
        &SourceInput::GitUrl {
            url: url.clone(),
            commit: "master".to_string(),
            auth: None,
            allow_branch_resolution: false,
        },
        work.path(),
    )
    .await
    .expect_err("branch should fail without opt-in");
    assert!(format!("{err}").contains("BranchNameNotAllowed"));

    let forced = SourceResolver::resolve(
        &SourceInput::GitUrl {
            url,
            commit: "master".to_string(),
            auth: None,
            allow_branch_resolution: true,
        },
        work.path(),
    )
    .await
    .expect("forced resolve");

    assert!(
        forced.warnings.iter().any(
            |w| matches!(w, SourceWarning::BranchResolved { branch, .. } if branch == "master")
        )
    );
}

#[tokio::test]
async fn source_resolver_unpacks_tar_gz_archive() {
    let workspace = tempfile::tempdir().expect("workspace");
    fs::write(
        workspace.path().join("Cargo.toml"),
        "[workspace]\nmembers = [\"member\"]\nresolver = \"2\"\n",
    )
    .expect("write workspace");
    fs::create_dir_all(workspace.path().join("member/src")).expect("mkdir");
    fs::write(
        workspace.path().join("member/Cargo.toml"),
        "[package]\nname = \"member\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .expect("manifest");
    fs::write(
        workspace.path().join("member/src/lib.rs"),
        "pub fn ok() {}\n",
    )
    .expect("lib");

    let archive_dir = tempfile::tempdir().expect("archive dir");
    let archive_path = archive_dir.path().join("workspace.tar.gz");
    let tar_gz = fs::File::create(&archive_path).expect("archive create");
    let encoder = flate2::write::GzEncoder::new(tar_gz, flate2::Compression::default());
    let mut tar = tar::Builder::new(encoder);
    tar.append_dir_all("repo", workspace.path())
        .expect("append dir");
    tar.finish().expect("finish tar");
    let encoder = tar.into_inner().expect("recover encoder");
    encoder.finish().expect("finalize gzip stream");

    let work = tempfile::tempdir().expect("workdir");
    let resolved = SourceResolver::resolve(
        &SourceInput::Archive {
            path: archive_path.clone(),
        },
        work.path(),
    )
    .await
    .expect("resolve archive");

    assert!(resolved.source.local_path.join("Cargo.toml").exists());
    assert!(resolved.source.commit_hash.starts_with("archive:"));
    assert!(!resolved.source.content_hash.is_empty());
}

#[test]
fn config_parser_reports_multiple_validation_errors() {
    let raw = RawAuditConfig {
        source: RawSource {
            url: Some("https://github.com/example/repo".to_string()),
            local_path: Some("/tmp/repo".to_string()),
            commit: Some("main".to_string()),
        },
        scope: Some(RawScope {
            target_crates: Some(vec![]),
            exclude_crates: Some(vec![]),
            features: None,
        }),
        engines: Some(RawEngineConfig {
            crypto_zk: Some(true),
            distributed: Some(false),
        }),
        budget: Some(RawBudgetConfig {
            kani_timeout_secs: Some(0),
            z3_timeout_secs: Some(0),
            fuzz_duration_secs: Some(0),
            madsim_ticks: Some(0),
            max_llm_retries: Some(0),
            semantic_index_timeout_secs: Some(0),
        }),
    };

    let errors = ConfigParser::validate(raw).expect_err("validation must fail");

    assert!(
        errors
            .iter()
            .any(|e| matches!(e, ConfigError::BranchNameNotAllowed { .. }))
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, ConfigError::ConflictingOptions { .. }))
    );
    assert!(
        errors
            .iter()
            .filter(|e| matches!(e, ConfigError::InvalidBudgetValue { .. }))
            .count()
            >= 3
    );
}

#[test]
fn workspace_analyzer_detects_bench_and_fuzz_targets() {
    let root = tempfile::tempdir().expect("root");
    fs::write(
        root.path().join("Cargo.toml"),
        "[workspace]\nmembers = [\"crypto\", \"crypto-bench\", \"crypto-fuzz\"]\nresolver = \"2\"\n",
    )
    .expect("write root");

    for (name, extra) in [
        ("crypto", ""),
        ("crypto-bench", "[[bench]]\nname = \"bench\"\n"),
        (
            "crypto-fuzz",
            "[[bin]]\nname = \"fuzz_target\"\npath = \"src/main.rs\"\n",
        ),
    ] {
        let crate_dir = root.path().join(name);
        fs::create_dir_all(crate_dir.join("src")).expect("mkdir");
        fs::write(
            crate_dir.join("Cargo.toml"),
            format!(
                "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n{extra}"
            ),
        )
        .expect("manifest");
        fs::write(crate_dir.join("src/lib.rs"), "pub fn x() {}\n").expect("source");
    }

    let ws = WorkspaceAnalyzer::analyze(root.path()).expect("analyze");
    let exclusions = WorkspaceAnalyzer::suggest_exclusions(&ws);
    let names: Vec<_> = exclusions.iter().map(|s| s.crate_name.as_str()).collect();
    assert!(names.contains(&"crypto-bench"));
    assert!(names.contains(&"crypto-fuzz"));
}

#[test]
fn framework_detector_finds_halo2_sp1_cairo_and_asm_feature() {
    let root = tempfile::tempdir().expect("root");
    fs::write(
        root.path().join("Cargo.toml"),
        "[workspace]\nmembers = [\"zk\"]\nresolver = \"2\"\n",
    )
    .expect("root");
    let zk = root.path().join("zk");
    fs::create_dir_all(zk.join("src")).expect("mkdir");
    fs::write(
        zk.join("Cargo.toml"),
        "[package]\nname = \"zk\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[features]\ndefault=[]\nasm=[]\n",
    )
    .expect("manifest");
    fs::write(
        zk.join("src/lib.rs"),
        r#"
        use halo2_proofs::plonk::ConstraintSystem;
        fn configure(cs: &mut ConstraintSystem<u64>) { let _ = "Chip::configure"; }
        sp1_zkvm::entrypoint!(main);
        #[cfg(feature = "asm")]
        fn field_mul_asm() {}
        "#,
    )
    .expect("source");
    fs::write(zk.join("src/circuit.cairo"), "fn main() -> felt252 { 1 }").expect("cairo source");

    let ws = WorkspaceAnalyzer::analyze(root.path()).expect("analyze");
    let detection = FrameworkDetector::detect(&ws);
    assert!(
        detection
            .frameworks
            .iter()
            .any(|f| f.framework == audit_agent_core::finding::Framework::Halo2)
    );
    assert!(
        detection
            .frameworks
            .iter()
            .any(|f| f.framework == audit_agent_core::finding::Framework::SP1)
    );
    assert!(
        detection
            .frameworks
            .iter()
            .any(|f| f.framework == audit_agent_core::finding::Framework::Cairo)
    );
    assert!(
        detection
            .crypto_divergent_features
            .iter()
            .any(|f| f.feature_name == "asm")
    );
}

#[tokio::test]
async fn optional_input_parser_extracts_constraints_from_markdown() {
    let temp = tempfile::tempdir().expect("temp");
    let spec_path = temp.path().join("spec.md");
    fs::write(
        &spec_path,
        r#"
        x in [0, 10)
        nonce must be unique per batch
        field_a must equal field_b
        assert(x < 10)
        "#,
    )
    .expect("write spec");

    let parsed = OptionalInputParser::parse_spec(&spec_path)
        .await
        .expect("parse spec");
    assert!(parsed.extracted_constraints.len() >= 3);
    assert!(
        parsed
            .extracted_constraints
            .iter()
            .any(|c| matches!(c.structured, StructuredConstraint::Range { .. }))
    );
    assert!(parsed.extracted_constraints.iter().any(|c| matches!(
        c.structured,
        StructuredConstraint::Custom {
            target: CustomAssertionTarget::Rust,
            ..
        }
    )));
}

#[test]
fn summarize_optional_inputs_respects_no_llm_prose_flag() {
    let mut config = AuditConfig {
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
            target_crates: vec!["member".to_string()],
            excluded_crates: vec![],
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
            api_key_present: true,
            provider: Some("openai".to_string()),
            no_llm_prose: false,
        },
        output_dir: PathBuf::from("audit-output"),
    };

    let summary = summarize_optional_inputs(&config);
    assert!(summary.llm_prose_used);

    config.llm.no_llm_prose = true;
    let summary = summarize_optional_inputs(&config);
    assert!(!summary.llm_prose_used);
}

#[test]
fn confirmation_summary_serializes_to_json() {
    let summary = ConfirmationSummary {
        crates: vec![],
        frameworks: vec![],
        crypto_divergent_features: vec![],
        build_matrix: vec![],
        estimated_duration_mins: 42,
        warnings: vec![
            IntakeWarning::LlmKeyMissing {
                degraded_features: vec!["Spec normalization".to_string()],
            },
            IntakeWarning::PreviousAuditParsed {
                prior_finding_count: 3,
            },
        ],
    };

    let json = WorkspaceConfirmation::to_json(&summary).expect("to json");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid json");
    assert_eq!(parsed["estimated_duration_mins"], 42);
}

#[test]
fn config_parser_parse_reports_all_bad_fields() {
    let temp = tempfile::tempdir().expect("temp");
    let config_path = temp.path().join("audit.yaml");
    fs::write(
        &config_path,
        r#"
source:
  url: https://github.com/example/repo
  local_path: /tmp/repo
  commit: main
budget:
  kani_timeout_secs: 0
  z3_timeout_secs: 0
  fuzz_duration_secs: 0
  madsim_ticks: 0
  max_llm_retries: 0
"#,
    )
    .expect("write yaml");

    let errors = ConfigParser::parse(&config_path).expect_err("should fail");
    let mut counts = HashMap::new();
    for err in &errors {
        *counts.entry(std::mem::discriminant(err)).or_insert(0usize) += 1;
    }
    assert!(errors.len() >= 5);
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, ConfigError::BranchNameNotAllowed { .. }))
    );
}
