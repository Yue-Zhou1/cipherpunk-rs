use audit_agent_core::finding::VerificationStatus;
use engine_distributed::feasibility::BridgeLevel;
use engine_distributed::verification::verification_status_for_distributed_run;

#[test]
fn level_a_with_trace_is_verified() {
    let status = verification_status_for_distributed_run(&BridgeLevel::LevelA, true);
    assert_eq!(status, VerificationStatus::Verified);
}

#[test]
fn level_c_is_unverified_black_box_even_with_trace() {
    let status = verification_status_for_distributed_run(
        &BridgeLevel::LevelC {
            reason: "runtime fragmentation".to_string(),
        },
        true,
    );
    match status {
        VerificationStatus::Unverified { reason } => {
            assert!(reason.contains("Level C black-box"));
        }
        other => panic!("expected unverified for Level C, got {other:?}"),
    }
}
