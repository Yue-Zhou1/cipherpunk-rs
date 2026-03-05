use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;

use audit_agent_core::finding::{
    CodeLocation, Evidence, Finding, FindingCategory, FindingId, FindingStatus, Framework,
    Severity, VerificationStatus,
};
use intake::diff::{AnalysisCache, DiffModeAnalyzer};
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

fn commit_hash(dir: &PathBuf) -> String {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(dir)
        .output()
        .expect("rev-parse");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn sample_finding(crate_name: &str) -> Finding {
    Finding {
        id: FindingId::new(format!("F-{crate_name}")),
        title: format!("finding for {crate_name}"),
        severity: Severity::Medium,
        category: FindingCategory::CryptoMisuse,
        framework: Framework::SP1,
        affected_components: vec![CodeLocation {
            crate_name: crate_name.to_string(),
            module: "lib".to_string(),
            file: PathBuf::from(format!("{crate_name}/src/lib.rs")),
            line_range: (1, 1),
            snippet: Some("unsafe_fn();".to_string()),
        }],
        prerequisites: String::new(),
        exploit_path: String::new(),
        impact: String::new(),
        evidence: Evidence {
            command: None,
            seed: None,
            trace_file: None,
            counterexample: None,
            harness_path: None,
            smt2_file: None,
            container_digest: "sha256:test".to_string(),
            tool_versions: HashMap::new(),
        },
        evidence_gate_level: 0,
        llm_generated: false,
        recommendation: String::new(),
        regression_test: None,
        status: FindingStatus::Open,
        regression_check: false,
        verification_status: VerificationStatus::Verified,
    }
}

fn setup_large_workspace() -> (TempDir, Vec<String>) {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path().to_path_buf();
    let members = (0..50)
        .map(|idx| format!("crate-{idx}"))
        .collect::<Vec<_>>();

    let manifest = format!(
        "[workspace]\nmembers = [{}]\nresolver = \"2\"\n",
        members
            .iter()
            .map(|name| format!("\"{name}\""))
            .collect::<Vec<_>>()
            .join(", ")
    );
    fs::write(root.join("Cargo.toml"), manifest).expect("write workspace manifest");

    for member in &members {
        let crate_dir = root.join(member);
        fs::create_dir_all(crate_dir.join("src")).expect("mkdir src");
        fs::write(
            crate_dir.join("Cargo.toml"),
            format!("[package]\nname = \"{member}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n"),
        )
        .expect("write crate manifest");
        fs::write(crate_dir.join("src/lib.rs"), "pub fn f() -> u8 { 1 }\n").expect("write source");
    }

    run(&root, "git", &["init", "-q"]);
    run(&root, "git", &["config", "user.name", "test"]);
    run(&root, "git", &["config", "user.email", "test@example.com"]);
    run(&root, "git", &["add", "."]);
    run(&root, "git", &["commit", "-qm", "base"]);
    (temp, members)
}

#[test]
fn two_file_change_reanalyzes_only_affected_crates_and_cache_hit_exceeds_80_percent() {
    let (workspace_dir, members) = setup_large_workspace();
    let root = workspace_dir.path().to_path_buf();
    let base = commit_hash(&root);

    fs::write(root.join("crate-3/src/lib.rs"), "pub fn f() -> u8 { 3 }\n").expect("modify crate-3");
    fs::write(
        root.join("crate-17/src/lib.rs"),
        "pub fn f() -> u8 { 17 }\n",
    )
    .expect("modify crate-17");
    run(&root, "git", &["add", "."]);
    run(&root, "git", &["commit", "-qm", "change two crates"]);
    let head = commit_hash(&root);

    let workspace = WorkspaceAnalyzer::analyze(&root).expect("analyze workspace");
    let cache = Arc::new(AnalysisCache::default());
    cache.insert(
        &base,
        &members
            .iter()
            .map(|name| sample_finding(name))
            .collect::<Vec<_>>(),
    );

    let analyzer = DiffModeAnalyzer::new(root.clone(), workspace, cache);
    let diff = analyzer.compute_diff(&base, &head).expect("compute diff");

    assert!(!diff.full_rerun_required);
    assert_eq!(diff.affected_crates.len(), 2);
    assert_eq!(diff.rerun_tasks.len(), 2);
    assert!(
        diff.rerun_tasks
            .iter()
            .all(|task| matches!(task, intake::diff::TaskId::AnalyzeFile(_))),
        "incremental diff should schedule per-file analysis tasks"
    );
    assert!(
        diff.cache_hit_rate > 0.80,
        "expected >80% cache hit, got {}",
        diff.cache_hit_rate
    );
}

#[test]
fn cargo_toml_change_triggers_full_rerun() {
    let (workspace_dir, _members) = setup_large_workspace();
    let root = workspace_dir.path().to_path_buf();
    let base = commit_hash(&root);

    let root_manifest = fs::read_to_string(root.join("Cargo.toml")).expect("read root Cargo.toml");
    fs::write(
        root.join("Cargo.toml"),
        format!("{root_manifest}\n# diff-mode full-rerun trigger\n"),
    )
    .expect("write root Cargo.toml");
    run(&root, "git", &["add", "."]);
    run(&root, "git", &["commit", "-qm", "touch cargo toml"]);
    let head = commit_hash(&root);

    let workspace = WorkspaceAnalyzer::analyze(&root).expect("analyze workspace");
    let analyzer =
        DiffModeAnalyzer::new(root.clone(), workspace, Arc::new(AnalysisCache::default()));
    let diff = analyzer.compute_diff(&base, &head).expect("compute diff");

    assert!(diff.full_rerun_required);
    assert_eq!(diff.rerun_tasks.len(), 50);
    assert!(
        diff.rerun_tasks
            .iter()
            .all(|task| matches!(task, intake::diff::TaskId::AnalyzeCrate(_))),
        "full rerun should schedule per-crate analysis tasks"
    );
}

#[test]
fn analysis_cache_persists_findings_across_instances() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cache_path = dir.path().join("analysis-cache.sled");
    let finding = sample_finding("crate-1");

    {
        let cache = AnalysisCache::open(&cache_path).expect("open persistent cache");
        cache.insert("base-commit", &[finding.clone()]);
    }

    {
        let cache = AnalysisCache::open(&cache_path).expect("re-open persistent cache");
        let loaded = cache.get("base-commit");
        assert_eq!(loaded.len(), 1, "cached findings should persist on disk");
        assert_eq!(loaded[0].id, finding.id);
    }
}
