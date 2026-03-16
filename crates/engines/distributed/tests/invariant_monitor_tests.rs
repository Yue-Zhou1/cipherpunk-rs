use std::collections::HashMap;

use audit_agent_core::audit_config::CustomInvariant;
use audit_agent_core::finding::Severity;
use engine_distributed::chaos::{InvariantId, NodeId};
use engine_distributed::invariants::{GlobalInvariantMonitor, NodeStateSnapshot, SimulationState};
use engine_distributed::trace::{EventKind, SimEvent};
use serde_json::json;

fn sample_state() -> SimulationState {
    let mut nodes = HashMap::<NodeId, NodeStateSnapshot>::new();
    nodes.insert(
        0,
        NodeStateSnapshot {
            committed_height: 100,
            committed_value: Some("A".to_string()),
            last_progress_tick: 10,
            processed_message_ids: vec!["m1".to_string(), "m2".to_string()],
            forced_withdrawal_reachable: true,
            forced_withdrawal_blocked_ticks: 0,
            state_root: Some("root-a".to_string()),
            finalized_roots: vec!["final-1".to_string()],
            revoked_finalized_roots: vec![],
        },
    );
    nodes.insert(
        1,
        NodeStateSnapshot {
            committed_height: 100,
            committed_value: Some("A".to_string()),
            last_progress_tick: 10,
            processed_message_ids: vec!["m3".to_string()],
            forced_withdrawal_reachable: true,
            forced_withdrawal_blocked_ticks: 0,
            state_root: Some("root-a".to_string()),
            finalized_roots: vec!["final-1".to_string()],
            revoked_finalized_roots: vec![],
        },
    );

    SimulationState {
        seed: 123,
        tick: 20,
        trace: vec![SimEvent {
            tick: 20,
            kind: EventKind::InvariantCheck,
            node: 0,
            payload: json!({"invariant": "safety"}),
        }],
        nodes,
        expected_progress_within_ticks: 50,
        escape_hatch_max_blocked_ticks: 30,
        liveness_exempt_nodes: vec![],
        custom_metrics: HashMap::new(),
    }
}

#[tokio::test]
async fn safety_invariant_fires_on_conflicting_commits() {
    let mut state = sample_state();
    state.nodes.get_mut(&1).expect("node").committed_value = Some("B".to_string());

    let monitor = GlobalInvariantMonitor::new_default();
    let violations = monitor.check_all(&state).await;

    assert!(
        violations
            .iter()
            .any(|v| v.invariant_id == InvariantId::Safety),
        "expected safety violation"
    );
}

#[tokio::test]
async fn escape_hatch_invariant_fires_when_blocked_too_long() {
    let mut state = sample_state();
    state
        .nodes
        .get_mut(&0)
        .expect("node")
        .forced_withdrawal_reachable = false;
    state
        .nodes
        .get_mut(&0)
        .expect("node")
        .forced_withdrawal_blocked_ticks = 100;
    state.escape_hatch_max_blocked_ticks = 40;

    let monitor = GlobalInvariantMonitor::new_default();
    let violations = monitor.check_all(&state).await;

    assert!(
        violations
            .iter()
            .any(|v| v.invariant_id == InvariantId::EscapeHatch),
        "expected escape hatch violation"
    );
}

#[tokio::test]
async fn custom_invariant_from_input_fires() {
    let mut state = sample_state();
    state.custom_metrics.insert(
        "ticks_since_last_batch".to_string(),
        serde_json::Value::from(999),
    );

    let custom = CustomInvariant {
        id: "INV-001".to_string(),
        name: "Batch finalization deadline".to_string(),
        description: "Sequencer must submit a batch within 12h".to_string(),
        check_expr: "ticks_since_last_batch <= 600".to_string(),
        violation_severity: Severity::High,
        spec_ref: None,
    };

    let monitor = GlobalInvariantMonitor::new_default().with_custom_invariants(&[custom]);
    let violations = monitor.check_all(&state).await;

    assert!(
        violations.iter().any(|v| matches!(
            &v.invariant_id,
            InvariantId::Custom(id) if id == "INV-001"
        )),
        "expected custom invariant violation"
    );
}

#[tokio::test]
async fn violation_evidence_keeps_seed_and_trace_for_reproduction() {
    let mut state = sample_state();
    state.nodes.get_mut(&1).expect("node").committed_value = Some("B".to_string());

    let monitor = GlobalInvariantMonitor::new_default();
    let violations = monitor.check_all(&state).await;
    let safety = violations
        .iter()
        .find(|v| v.invariant_id == InvariantId::Safety)
        .expect("safety violation");

    assert_eq!(safety.evidence.seed, 123);
    assert!(!safety.evidence.event_trace.is_empty());
    assert!(!safety.evidence.node_states.is_empty());
}

#[tokio::test]
async fn invariant_violation_can_emit_replay_bundle_files() {
    let mut state = sample_state();
    state.nodes.get_mut(&1).expect("node").committed_value = Some("B".to_string());

    let monitor = GlobalInvariantMonitor::new_default();
    let violations = monitor.check_all(&state).await;
    let safety = violations
        .iter()
        .find(|v| v.invariant_id == InvariantId::Safety)
        .expect("safety violation");

    let dir = tempfile::tempdir().expect("tempdir");
    let traces_dir = safety
        .write_reproduction_bundle(
            dir.path(),
            "FINDING-ABC",
            std::path::Path::new("harness"),
            "ghcr.io/audit/madsim@sha256:beef",
        )
        .expect("write reproduction bundle");

    assert!(traces_dir.join("trace.json").exists());
    assert!(traces_dir.join("replay.sh").exists());
}

#[tokio::test]
async fn liveness_invariant_fires_when_single_non_exempt_node_stalls() {
    let mut state = sample_state();
    state.tick = 1_000;
    state.expected_progress_within_ticks = 50;
    state.nodes.get_mut(&0).expect("node").last_progress_tick = 980;
    state.nodes.get_mut(&1).expect("node").last_progress_tick = 1;

    let monitor = GlobalInvariantMonitor::new_default();
    let violations = monitor.check_all(&state).await;

    assert!(
        violations
            .iter()
            .any(|v| v.invariant_id == InvariantId::Liveness),
        "single stalled non-exempt node should trip liveness"
    );
}

#[tokio::test]
async fn liveness_invariant_ignores_exempt_nodes() {
    let mut state = sample_state();
    state.tick = 1_000;
    state.expected_progress_within_ticks = 50;
    state.nodes.get_mut(&0).expect("node").last_progress_tick = 980;
    state.nodes.get_mut(&1).expect("node").last_progress_tick = 1;
    state.liveness_exempt_nodes = vec![1];

    let monitor = GlobalInvariantMonitor::new_default();
    let violations = monitor.check_all(&state).await;

    assert!(
        !violations
            .iter()
            .any(|v| v.invariant_id == InvariantId::Liveness),
        "exempt stalled node should not trigger liveness"
    );
}
