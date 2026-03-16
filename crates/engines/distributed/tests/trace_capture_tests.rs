use std::process::Command;

use engine_distributed::trace::{EventKind, SimEvent, TraceCapture};
use serde_json::json;
use tempfile::tempdir;

fn sample_events(count: usize) -> Vec<SimEvent> {
    (0..count)
        .map(|idx| SimEvent {
            tick: idx as u64,
            kind: EventKind::MessageSent,
            node: (idx % 4) as u32,
            payload: json!({
                "message_id": format!("m-{idx}"),
                "kind": "message_sent"
            }),
        })
        .collect()
}

#[test]
fn trace_json_is_byte_identical_for_same_seed_and_events() {
    let events = sample_events(10);
    let a = TraceCapture {
        seed: 42,
        events: events.clone(),
        duration_ticks: 10,
    };
    let b = TraceCapture {
        seed: 42,
        events,
        duration_ticks: 10,
    };

    assert_eq!(a.to_json(), b.to_json());
}

#[test]
fn shrink_reduces_large_trace_under_50_events_and_keeps_violation_tick() {
    let capture = TraceCapture {
        seed: 9,
        events: sample_events(10_000),
        duration_ticks: 10_000,
    };
    let shrunk = capture.shrink(9_000);

    assert!(shrunk.events.len() < 50);
    assert!(shrunk.events.iter().any(|event| event.tick == 9_000));
}

#[test]
fn replay_script_uses_exact_container_digest() {
    let capture = TraceCapture {
        seed: 7,
        events: sample_events(2),
        duration_ticks: 2,
    };
    let digest = "ghcr.io/audit/madsim@sha256:abc123";
    let replay = capture.to_replay_script(std::path::Path::new("harness"), digest);

    assert!(replay.contains(digest));
    assert!(replay.contains("--seed 7"));
}

#[test]
fn regression_test_snippet_is_compilable_without_llm() {
    let capture = TraceCapture {
        seed: 15,
        events: sample_events(3),
        duration_ticks: 3,
    };
    let source = capture.to_regression_test("seed_replay_regression");

    let dir = tempdir().expect("tempdir");
    let test_file = dir.path().join("generated_regression.rs");
    std::fs::write(&test_file, source).expect("write generated test");

    let output = Command::new("rustc")
        .arg("--edition=2024")
        .arg("--test")
        .arg(&test_file)
        .arg("-o")
        .arg(dir.path().join("generated_regression_bin"))
        .output()
        .expect("invoke rustc");

    assert!(
        output.status.success(),
        "generated regression test should compile: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn evidence_pack_traces_and_regression_files_are_written() {
    let capture = TraceCapture {
        seed: 21,
        events: sample_events(5),
        duration_ticks: 5,
    };
    let dir = tempdir().expect("tempdir");

    let traces_dir = capture
        .write_evidence_files(
            dir.path(),
            "FINDING-001",
            std::path::Path::new("harness"),
            "ghcr.io/audit/madsim@sha256:cafe",
        )
        .expect("write evidence files");
    assert!(traces_dir.join("trace.json").exists());
    assert!(traces_dir.join("seed.txt").exists());
    assert!(traces_dir.join("replay.sh").exists());

    let regression_dir = dir.path().join("regression-tests/madsim_scenarios");
    let test_path = capture
        .write_regression_test("seed_replay_21", &regression_dir)
        .expect("write regression test");
    assert!(test_path.exists());
}
