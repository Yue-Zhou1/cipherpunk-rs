use engine_distributed::chaos::{
    ChaosScript, ChaosStep, InvariantId, ScenarioRunner, SyntheticConsensusFixture,
    load_builtin_scenario,
};

fn partition_then_safety_check_script() -> ChaosScript {
    ChaosScript {
        name: "partition-safety-check".to_string(),
        description: "trigger safety check under partition".to_string(),
        steps: vec![
            ChaosStep::Partition {
                nodes: vec![2, 3],
                duration_ticks: 1_000,
            },
            ChaosStep::CheckInvariant {
                invariant: InvariantId::Safety,
            },
        ],
    }
}

#[test]
fn partition_scenario_triggers_safety_violation_on_broken_fixture() {
    let script = partition_then_safety_check_script();
    let runner = ScenarioRunner::new(7, SyntheticConsensusFixture::broken_safety());

    let outcome = runner.run(&script);
    assert!(
        outcome
            .invariant_violations
            .iter()
            .any(|violation| violation.invariant == InvariantId::Safety),
        "expected safety invariant violation for broken fixture"
    );
}

#[test]
fn scenarios_serialize_to_json_and_back() {
    let script = partition_then_safety_check_script();

    let json = script.to_json().expect("serialize scenario to json");
    let restored = ChaosScript::from_json(&json).expect("deserialize scenario from json");

    assert_eq!(restored, script);
}

#[test]
fn same_json_and_seed_produce_identical_trace_output() {
    let script = partition_then_safety_check_script();
    let json = script.to_json().expect("serialize script");
    let restored = ChaosScript::from_json(&json).expect("deserialize script");

    let runner_a = ScenarioRunner::new(42, SyntheticConsensusFixture::broken_safety());
    let runner_b = ScenarioRunner::new(42, SyntheticConsensusFixture::broken_safety());

    let out_a = runner_a.run(&restored);
    let out_b = runner_b.run(&restored);

    assert_eq!(out_a.trace, out_b.trace);
    assert_eq!(out_a.invariant_violations, out_b.invariant_violations);
}

#[test]
fn builtin_templates_are_loadable_yaml_scripts() {
    for file_name in [
        "partition-then-rejoin.yaml",
        "byzantine-double-vote.yaml",
        "eclipse-attack.yaml",
    ] {
        let script = load_builtin_scenario(file_name).expect("load built-in scenario");
        assert!(
            !script.steps.is_empty(),
            "built-in scenario should include steps"
        );
    }
}

#[test]
fn eclipse_template_uses_liveness_except_nodes_shape() {
    let raw = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("scenarios/eclipse-attack.yaml"),
    )
    .expect("read eclipse template");
    assert!(
        raw.contains("liveness_except_nodes"),
        "eclipse scenario must use liveness_except_nodes invariant form"
    );
}

#[test]
fn liveness_except_nodes_skips_violation_when_only_exempt_node_stalls() {
    let script = load_builtin_scenario("eclipse-attack.yaml").expect("load eclipse scenario");
    let fixture = SyntheticConsensusFixture::broken_safety_and_liveness();
    let outcome = fixture.run_with_seed(33, &script);

    assert!(
        !outcome.has_liveness_violation(),
        "exempt-node liveness check should not fail when only exempt nodes are isolated"
    );
}

#[test]
fn refuse_sync_step_roundtrips_through_json() {
    let script = ChaosScript {
        name: "refuse-sync-roundtrip".to_string(),
        description: "roundtrip RefuseSync".to_string(),
        steps: vec![ChaosStep::RefuseSync {
            node: 1,
            for_heights: 10..=20,
        }],
    };

    let json = script.to_json().expect("serialize script");
    let restored = ChaosScript::from_json(&json).expect("deserialize script");
    assert_eq!(restored, script);
}
