use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use audit_agent_core::audit_config::BudgetConfig;
use engine_crypto::semantic::ra_client::SemanticIndex;
use engine_crypto::zk::halo2::cdg::{ConstraintDependencyGraph, RiskAnnotation};
use intake::workspace::WorkspaceAnalyzer;
use tempfile::{TempDir, tempdir};

fn write_file(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent directory");
    }
    fs::write(path, content).expect("write file");
}

fn budget() -> BudgetConfig {
    BudgetConfig {
        kani_timeout_secs: 300,
        z3_timeout_secs: 600,
        fuzz_duration_secs: 3600,
        madsim_ticks: 100_000,
        max_llm_retries: 3,
        semantic_index_timeout_secs: 5,
    }
}

fn build_semantic_index_from_fixture(source: &str) -> (TempDir, SemanticIndex) {
    let dir = tempdir().expect("tempdir");
    write_file(
        &dir.path().join("Cargo.toml"),
        r#"[workspace]
members = ["zk"]
resolver = "2"
"#,
    );
    write_file(
        &dir.path().join("zk/Cargo.toml"),
        r#"[package]
name = "zk"
version = "0.1.0"
edition = "2024"
"#,
    );
    write_file(&dir.path().join("zk/src/lib.rs"), source);

    let workspace = WorkspaceAnalyzer::analyze(dir.path()).expect("analyze workspace");

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build runtime");
    let index = runtime
        .block_on(SemanticIndex::build(&workspace, &budget()))
        .expect("build semantic index");
    (dir, index)
}

#[test]
fn cdg_build_identifies_chip_nodes_and_method_spans() {
    let (_tmp, index) = build_semantic_index_from_fixture(
        r#"
pub trait Chip {
    fn configure(meta: &mut usize);
    fn synthesize();
}

pub struct RangeCheckChip;
impl Chip for RangeCheckChip {
    fn configure(meta: &mut usize) {
        let _value = meta;
    }
    fn synthesize() {}
}

pub struct BalanceChip;
impl Chip for BalanceChip {
    fn configure(meta: &mut usize) {
        RangeCheckChip::configure(meta);
    }
    fn synthesize() {}
}
"#,
    );

    let cdg = ConstraintDependencyGraph::build(&index).expect("build cdg");

    assert!(cdg.chips.iter().any(|chip| {
        chip.name == "RangeCheckChip"
            && chip.configure_span.is_some()
            && chip.synthesize_span.is_some()
    }));
    assert!(
        cdg.chips
            .iter()
            .any(|chip| chip.name == "BalanceChip" && chip.configure_span.is_some()),
        "chips: {:?}",
        cdg.chips
    );
}

#[test]
fn cdg_builds_edges_between_rangecheck_and_consumers() {
    let (_tmp, index) = build_semantic_index_from_fixture(
        r#"
pub trait Chip {
    fn configure(meta: &mut usize);
}

pub struct RangeCheckChip;
impl Chip for RangeCheckChip {
    fn configure(_meta: &mut usize) {}
}

pub struct BalanceChip;
impl Chip for BalanceChip {
    fn configure(meta: &mut usize) {
        RangeCheckChip::configure(meta);
    }
}
"#,
    );

    let cdg = ConstraintDependencyGraph::build(&index).expect("build cdg");

    assert!(
        cdg.edges
            .iter()
            .any(|edge| edge.from_chip == "RangeCheckChip" && edge.to_chip == "BalanceChip"),
        "edges: {:?}",
        cdg.edges
    );
}

#[test]
fn isolated_node_annotation_fires_for_unconstrained_column_fixture() {
    let (_tmp, index) = build_semantic_index_from_fixture(
        r#"
pub trait Chip {
    fn configure(meta: &mut Meta);
}

pub struct Meta;
impl Meta {
    pub fn advice_column(&mut self) -> &'static str { "advice_a" }
    pub fn selector(&mut self) -> &'static str { "q_enable" }
}

pub struct LooseChip;
impl Chip for LooseChip {
    fn configure(meta: &mut Meta) {
        let advice_a = meta.advice_column();
        let _q_enable = meta.selector();
        let _ = advice_a;
    }
}
"#,
    );

    let cdg = ConstraintDependencyGraph::build(&index).expect("build cdg");

    assert!(cdg.risk_annotations.iter().any(|annotation| matches!(
        annotation,
        RiskAnnotation::IsolatedNode { chip, column } if chip == "LooseChip" && column == "advice_a"
    )));
}

#[test]
fn cdg_serialization_outputs_dot_and_json() {
    let (_tmp, index) = build_semantic_index_from_fixture(
        r#"
pub trait Chip {
    fn configure(meta: &mut usize);
}

pub struct RangeCheckChip;
impl Chip for RangeCheckChip {
    fn configure(_meta: &mut usize) {}
}
"#,
    );

    let cdg = ConstraintDependencyGraph::build(&index).expect("build cdg");
    let dot = cdg.to_dot();
    let json = cdg.to_json();

    assert!(dot.contains("digraph cdg"));
    assert!(dot.contains("RangeCheckChip"));
    assert!(json.contains("\"chips\""));
    assert!(json.contains("RangeCheckChip"));
}

#[test]
fn cdg_dot_renders_with_graphviz_container_when_available() {
    let docker_ok = Command::new("docker")
        .args(["info"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !docker_ok {
        return;
    }

    let (_tmp, index) = build_semantic_index_from_fixture(
        r#"
pub trait Chip {
    fn configure(meta: &mut usize);
}

pub struct RangeCheckChip;
impl Chip for RangeCheckChip {
    fn configure(_meta: &mut usize) {}
}
"#,
    );
    let cdg = ConstraintDependencyGraph::build(&index).expect("build cdg");
    let dot = cdg.to_dot();

    let mut child = Command::new("docker")
        .args([
            "run",
            "--rm",
            "-i",
            "--network",
            "none",
            "minidocks/graphviz",
            "dot",
            "-Tsvg",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn graphviz container");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(dot.as_bytes())
        .expect("write dot input");
    let output = child.wait_with_output().expect("wait graphviz");

    assert!(
        output.status.success(),
        "graphviz render failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("<svg"),
        "expected svg output from graphviz"
    );
}
