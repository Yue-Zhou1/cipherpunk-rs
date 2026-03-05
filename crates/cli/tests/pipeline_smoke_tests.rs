use std::path::{Path, PathBuf};
use std::process::Command;

use audit_agent_cli::{AnalyzeArgs, DiffArgs, run_analyze, run_diff};
use tempfile::tempdir;

fn write_file(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create parent directory");
    }
    std::fs::write(path, content).expect("write file");
}

fn git(repo_root: &Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(args)
        .status()
        .expect("run git");
    assert!(status.success(), "git command failed: {:?}", args);
}

fn git_output(repo_root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(args)
        .output()
        .expect("run git output");
    assert!(
        output.status.success(),
        "git output command failed: {:?}",
        args
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn cargo_generate_lockfile(repo_root: &Path) {
    let status = Command::new("cargo")
        .arg("generate-lockfile")
        .arg("--manifest-path")
        .arg(repo_root.join("Cargo.toml"))
        .status()
        .expect("run cargo generate-lockfile");
    assert!(status.success(), "cargo generate-lockfile failed");
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates/")
        .parent()
        .expect("repo root")
        .to_path_buf()
}

#[tokio::test]
async fn analyze_runs_intake_through_orchestrator_and_writes_outputs() {
    let dir = tempdir().expect("tempdir");
    let workspace = dir.path().join("audit-target");

    write_file(
        &workspace.join("Cargo.toml"),
        r#"[workspace]
members = ["rollup-core"]
resolver = "2"
"#,
    );
    write_file(
        &workspace.join("rollup-core/Cargo.toml"),
        r#"[package]
name = "rollup-core"
version = "0.1.0"
edition = "2024"
"#,
    );
    write_file(
        &workspace.join("rollup-core/src/lib.rs"),
        r#"
pub fn verify() {
    let msg = b"hello";
    let _digest = transcript_hash_no_domain(msg);
}

fn transcript_hash_no_domain(_msg: &[u8]) -> [u8; 32] {
    [0; 32]
}
"#,
    );
    cargo_generate_lockfile(&workspace);

    let audit_yaml = dir.path().join("audit.yaml");
    write_file(
        &audit_yaml,
        &format!(
            r#"source:
  local_path: "{}"
engines:
  crypto_zk: true
  distributed: false
budget:
  kani_timeout_secs: 10
  z3_timeout_secs: 10
  fuzz_duration_secs: 10
  madsim_ticks: 1000
  max_llm_retries: 1
  semantic_index_timeout_secs: 10
"#,
            workspace.display()
        ),
    );

    git(&workspace, &["init"]);
    git(&workspace, &["add", "."]);
    git(
        &workspace,
        &[
            "-c",
            "user.email=audit@example.com",
            "-c",
            "user.name=Audit Agent",
            "commit",
            "-m",
            "init",
        ],
    );

    let output_dir = dir.path().join("output");
    let args = AnalyzeArgs {
        audit_yaml,
        git_url: None,
        local_path: Some(workspace.clone()),
        archive: None,
        commit: None,
        allow_branch_resolution: false,
        git_token: None,
        work_dir: dir.path().join("work"),
        spec: None,
        prev_audit: None,
        invariants: None,
        entries: None,
        output_dir: Some(output_dir.clone()),
        evidence_pack_zip: Some(dir.path().join("seed-evidence-pack.zip")),
        rules_dir: repo_root().join("rules"),
        no_llm_prose: true,
    };

    let outputs = run_analyze(args).await.expect("run analyze");
    assert!(
        outputs.findings.iter().any(|finding| {
            finding.id.to_string().contains("CRYPTO-002")
                || finding.title.contains("domain separator")
        }),
        "expected at least one CRYPTO-002 style finding"
    );

    for file in [
        "report-executive.md",
        "report-technical.md",
        "audit-manifest.json",
        "findings.json",
        "findings.sarif",
        "evidence-pack.zip",
    ] {
        assert!(
            output_dir.join(file).exists(),
            "expected output file {}",
            file
        );
    }
}

#[test]
fn diff_reports_changed_files_and_crates() {
    let dir = tempdir().expect("tempdir");
    let workspace = dir.path().join("diff-target");

    write_file(
        &workspace.join("Cargo.toml"),
        r#"[workspace]
members = ["rollup-core"]
resolver = "2"
"#,
    );
    write_file(
        &workspace.join("rollup-core/Cargo.toml"),
        r#"[package]
name = "rollup-core"
version = "0.1.0"
edition = "2024"
"#,
    );
    write_file(
        &workspace.join("rollup-core/src/lib.rs"),
        "pub fn value() -> u64 { 1 }\n",
    );
    cargo_generate_lockfile(&workspace);

    git(&workspace, &["init"]);
    git(&workspace, &["add", "."]);
    git(
        &workspace,
        &[
            "-c",
            "user.email=audit@example.com",
            "-c",
            "user.name=Audit Agent",
            "commit",
            "-m",
            "base",
        ],
    );
    let base = git_output(&workspace, &["rev-parse", "HEAD"]);

    write_file(
        &workspace.join("rollup-core/src/lib.rs"),
        "pub fn value() -> u64 { 2 }\n",
    );
    git(&workspace, &["add", "rollup-core/src/lib.rs"]);
    git(
        &workspace,
        &[
            "-c",
            "user.email=audit@example.com",
            "-c",
            "user.name=Audit Agent",
            "commit",
            "-m",
            "head",
        ],
    );
    let head = git_output(&workspace, &["rev-parse", "HEAD"]);

    let diff = run_diff(DiffArgs {
        repo_root: workspace,
        base,
        head,
        cache_dir: None,
    })
    .expect("run diff");
    assert!(!diff.full_rerun_required);
    assert_eq!(diff.affected_crates, vec!["rollup-core".to_string()]);
    assert_eq!(
        diff.affected_files,
        vec![PathBuf::from("rollup-core/src/lib.rs")]
    );
}
