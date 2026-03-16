use audit_agent_core::finding::VerificationStatus;

use crate::feasibility::BridgeLevel;

pub fn verification_status_for_distributed_run(
    bridge_level: &BridgeLevel,
    trace_captured: bool,
) -> VerificationStatus {
    match bridge_level {
        BridgeLevel::LevelC { reason } => VerificationStatus::Unverified {
            reason: format!("Level C black-box simulation: {reason}"),
        },
        BridgeLevel::LevelA | BridgeLevel::LevelB { .. } if trace_captured => {
            VerificationStatus::Verified
        }
        BridgeLevel::LevelA | BridgeLevel::LevelB { .. } => VerificationStatus::Unverified {
            reason: "missing deterministic trace evidence".to_string(),
        },
    }
}
