use std::fs;
use std::path::Path;

use audit_agent_core::audit_config::BudgetConfig;
use engine_crypto::semantic::ra_client::SemanticIndex;
use engine_distributed::chaos::{
    BridgeScenarioResult, SyntheticConsensusFixture, load_builtin_scenario,
};
use engine_distributed::feasibility::{BridgeLevel, MadSimFeasibilityAssessor};
use engine_distributed::trace::{SimEvent, TraceCapture};
use intake::workspace::WorkspaceAnalyzer;
use tempfile::tempdir;

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

#[tokio::test]
async fn phase4_end_to_end_smoke() {
    let dir = tempdir().expect("tempdir");
    write_file(
        &dir.path().join("Cargo.toml"),
        r#"[workspace]
members = ["consensus-min"]
resolver = "2"
"#,
    );
    write_file(
        &dir.path().join("consensus-min/Cargo.toml"),
        r#"[package]
name = "consensus-min"
version = "0.1.0"
edition = "2024"
"#,
    );
    write_file(
        &dir.path().join("consensus-min/src/lib.rs"),
        r#"pub async fn start_node() {}
"#,
    );

    let workspace = WorkspaceAnalyzer::analyze(dir.path()).expect("analyze workspace");
    let semantic = SemanticIndex::build(&workspace, &budget())
        .await
        .expect("build semantic index");
    let level = MadSimFeasibilityAssessor::assess(&workspace, &semantic);
    assert_eq!(level, BridgeLevel::LevelA);

    let fixture = SyntheticConsensusFixture::broken_safety_and_liveness();
    let partition_script = load_builtin_scenario("partition-then-rejoin.yaml").expect("scenario");
    let byz_script = load_builtin_scenario("byzantine-double-vote.yaml").expect("scenario");

    let partition_outcome: BridgeScenarioResult = fixture.run_with_seed(77, &partition_script);
    assert!(
        partition_outcome.has_liveness_violation(),
        "partition scenario should trigger liveness violation on broken fixture"
    );

    let byz_outcome: BridgeScenarioResult = fixture.run_with_seed(77, &byz_script);
    assert!(
        byz_outcome.has_safety_violation(),
        "double vote scenario should trigger safety violation on broken fixture"
    );

    let capture = TraceCapture {
        seed: 77,
        events: partition_outcome
            .trace
            .iter()
            .cloned()
            .map(SimEvent::from)
            .collect(),
        duration_ticks: partition_outcome.max_tick(),
    };
    let replay = capture.to_replay_script(
        Path::new("generated-harness"),
        "ghcr.io/audit/madsim@sha256:1234",
    );
    assert!(replay.contains("--seed 77"));
    assert!(replay.contains("ghcr.io/audit/madsim@sha256:1234"));

    let big_capture = TraceCapture {
        seed: 77,
        events: (0..300)
            .flat_map(|_| capture.events.clone())
            .collect::<Vec<_>>(),
        duration_ticks: capture.duration_ticks.saturating_mul(300),
    };
    let shrunk = big_capture.shrink(partition_outcome.max_tick());
    assert!(shrunk.events.len() < 50);
}
