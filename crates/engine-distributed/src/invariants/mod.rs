use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::Result;
use async_trait::async_trait;
use audit_agent_core::audit_config::CustomInvariant;
use serde_json::Value;

use crate::chaos::{InvariantId, NodeId};
use crate::trace::{SimEvent, TraceCapture};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeStateSnapshot {
    pub committed_height: u64,
    pub committed_value: Option<String>,
    pub last_progress_tick: u64,
    pub processed_message_ids: Vec<String>,
    pub forced_withdrawal_reachable: bool,
    pub forced_withdrawal_blocked_ticks: u64,
    pub state_root: Option<String>,
    pub finalized_roots: Vec<String>,
    pub revoked_finalized_roots: Vec<String>,
}

impl Default for NodeStateSnapshot {
    fn default() -> Self {
        Self {
            committed_height: 0,
            committed_value: None,
            last_progress_tick: 0,
            processed_message_ids: Vec::new(),
            forced_withdrawal_reachable: true,
            forced_withdrawal_blocked_ticks: 0,
            state_root: None,
            finalized_roots: Vec::new(),
            revoked_finalized_roots: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimulationState {
    pub seed: u64,
    pub tick: u64,
    pub trace: Vec<SimEvent>,
    pub nodes: HashMap<NodeId, NodeStateSnapshot>,
    pub expected_progress_within_ticks: u64,
    pub escape_hatch_max_blocked_ticks: u64,
    pub liveness_exempt_nodes: Vec<NodeId>,
    pub custom_metrics: HashMap<String, Value>,
}

impl Default for SimulationState {
    fn default() -> Self {
        Self {
            seed: 0,
            tick: 0,
            trace: Vec::new(),
            nodes: HashMap::new(),
            expected_progress_within_ticks: 1_000,
            escape_hatch_max_blocked_ticks: 1_000,
            liveness_exempt_nodes: Vec::new(),
            custom_metrics: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViolationEvidence {
    pub seed: u64,
    pub event_trace: Vec<SimEvent>,
    pub node_states: HashMap<NodeId, NodeStateSnapshot>,
}

impl ViolationEvidence {
    pub fn to_trace_capture(&self, duration_ticks: u64) -> TraceCapture {
        TraceCapture {
            seed: self.seed,
            events: self.event_trace.clone(),
            duration_ticks,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvariantViolation {
    pub invariant_id: InvariantId,
    pub description: String,
    pub violated_at_tick: u64,
    pub involved_nodes: Vec<NodeId>,
    pub evidence: ViolationEvidence,
}

impl InvariantViolation {
    pub fn write_reproduction_bundle(
        &self,
        evidence_pack_root: &Path,
        finding_id: &str,
        harness_path: &Path,
        container_image: &str,
    ) -> Result<PathBuf> {
        let duration_ticks = self
            .evidence
            .event_trace
            .last()
            .map(|event| event.tick)
            .unwrap_or(self.violated_at_tick);
        let capture = self.evidence.to_trace_capture(duration_ticks);
        capture.write_evidence_files(
            evidence_pack_root,
            finding_id,
            harness_path,
            container_image,
        )
    }
}

#[async_trait]
pub trait Invariant: Send + Sync {
    fn id(&self) -> InvariantId;
    fn name(&self) -> &str;
    async fn check(&self, state: &SimulationState) -> Option<InvariantViolation>;
}

pub struct SafetyInvariant;
pub struct LivenessInvariant;
pub struct IdempotencyInvariant;
pub struct EscapeHatchInvariant;
pub struct ProverConsistencyInvariant;
pub struct FinalizationMonotonicityInvariant;

pub struct CustomInvariantChecker {
    spec: CustomInvariant,
}

impl CustomInvariantChecker {
    pub fn new(spec: CustomInvariant) -> Self {
        Self { spec }
    }
}

#[async_trait]
impl Invariant for SafetyInvariant {
    fn id(&self) -> InvariantId {
        InvariantId::Safety
    }

    fn name(&self) -> &str {
        "SafetyInvariant"
    }

    async fn check(&self, state: &SimulationState) -> Option<InvariantViolation> {
        let mut by_height: HashMap<u64, (String, NodeId)> = HashMap::new();
        for (node_id, snapshot) in &state.nodes {
            let Some(value) = &snapshot.committed_value else {
                continue;
            };
            if let Some((existing_value, existing_node)) = by_height.get(&snapshot.committed_height)
            {
                if existing_value != value {
                    return Some(new_violation(
                        InvariantId::Safety,
                        "conflicting commits detected at same height".to_string(),
                        state,
                        vec![*existing_node, *node_id],
                    ));
                }
            } else {
                by_height.insert(snapshot.committed_height, (value.clone(), *node_id));
            }
        }
        None
    }
}

#[async_trait]
impl Invariant for LivenessInvariant {
    fn id(&self) -> InvariantId {
        InvariantId::Liveness
    }

    fn name(&self) -> &str {
        "LivenessInvariant"
    }

    async fn check(&self, state: &SimulationState) -> Option<InvariantViolation> {
        if state.nodes.is_empty() {
            return None;
        }
        let exempt: HashSet<NodeId> = state.liveness_exempt_nodes.iter().copied().collect();
        let stalled: Vec<NodeId> = state
            .nodes
            .iter()
            .filter_map(|(node_id, snapshot)| {
                if exempt.contains(node_id) {
                    return None;
                }
                if state.tick.saturating_sub(snapshot.last_progress_tick)
                    > state.expected_progress_within_ticks
                {
                    Some(*node_id)
                } else {
                    None
                }
            })
            .collect();
        if !stalled.is_empty() {
            return Some(new_violation(
                InvariantId::Liveness,
                "non-exempt nodes stalled beyond progress threshold".to_string(),
                state,
                stalled,
            ));
        }
        None
    }
}

#[async_trait]
impl Invariant for IdempotencyInvariant {
    fn id(&self) -> InvariantId {
        InvariantId::Idempotency
    }

    fn name(&self) -> &str {
        "IdempotencyInvariant"
    }

    async fn check(&self, state: &SimulationState) -> Option<InvariantViolation> {
        for (node_id, snapshot) in &state.nodes {
            let mut seen = HashSet::<String>::new();
            for message_id in &snapshot.processed_message_ids {
                if !seen.insert(message_id.clone()) {
                    return Some(new_violation(
                        InvariantId::Idempotency,
                        format!("duplicate message id `{message_id}` processed"),
                        state,
                        vec![*node_id],
                    ));
                }
            }
        }
        None
    }
}

#[async_trait]
impl Invariant for EscapeHatchInvariant {
    fn id(&self) -> InvariantId {
        InvariantId::EscapeHatch
    }

    fn name(&self) -> &str {
        "EscapeHatchInvariant"
    }

    async fn check(&self, state: &SimulationState) -> Option<InvariantViolation> {
        let violating: Vec<NodeId> = state
            .nodes
            .iter()
            .filter_map(|(node_id, snapshot)| {
                if !snapshot.forced_withdrawal_reachable
                    && snapshot.forced_withdrawal_blocked_ticks
                        > state.escape_hatch_max_blocked_ticks
                {
                    Some(*node_id)
                } else {
                    None
                }
            })
            .collect();

        if !violating.is_empty() {
            return Some(new_violation(
                InvariantId::EscapeHatch,
                "forced withdrawal unreachable beyond threshold".to_string(),
                state,
                violating,
            ));
        }
        None
    }
}

#[async_trait]
impl Invariant for ProverConsistencyInvariant {
    fn id(&self) -> InvariantId {
        InvariantId::ProverConsistency
    }

    fn name(&self) -> &str {
        "ProverConsistencyInvariant"
    }

    async fn check(&self, state: &SimulationState) -> Option<InvariantViolation> {
        let mut first_root: Option<(String, NodeId)> = None;
        let mut conflicting_nodes = Vec::new();
        for (node_id, snapshot) in &state.nodes {
            let Some(root) = &snapshot.state_root else {
                continue;
            };
            match &first_root {
                Some((first, first_node)) if first != root => {
                    conflicting_nodes.push(*first_node);
                    conflicting_nodes.push(*node_id);
                    return Some(new_violation(
                        InvariantId::ProverConsistency,
                        "state root mismatch across provers".to_string(),
                        state,
                        conflicting_nodes,
                    ));
                }
                None => {
                    first_root = Some((root.clone(), *node_id));
                }
                _ => {}
            }
        }
        None
    }
}

#[async_trait]
impl Invariant for FinalizationMonotonicityInvariant {
    fn id(&self) -> InvariantId {
        InvariantId::FinalizationMonotonicity
    }

    fn name(&self) -> &str {
        "FinalizationMonotonicityInvariant"
    }

    async fn check(&self, state: &SimulationState) -> Option<InvariantViolation> {
        for (node_id, snapshot) in &state.nodes {
            let finalized: HashSet<&String> = snapshot.finalized_roots.iter().collect();
            let revoked: Vec<&String> = snapshot
                .revoked_finalized_roots
                .iter()
                .filter(|root| finalized.contains(root))
                .collect();
            if !revoked.is_empty() {
                return Some(new_violation(
                    InvariantId::FinalizationMonotonicity,
                    "previously finalized root was revoked".to_string(),
                    state,
                    vec![*node_id],
                ));
            }
        }
        None
    }
}

#[async_trait]
impl Invariant for CustomInvariantChecker {
    fn id(&self) -> InvariantId {
        InvariantId::Custom(self.spec.id.clone())
    }

    fn name(&self) -> &str {
        &self.spec.name
    }

    async fn check(&self, state: &SimulationState) -> Option<InvariantViolation> {
        evaluate_custom_expr(state, &self.spec).map(|description| {
            new_violation(
                InvariantId::Custom(self.spec.id.clone()),
                description,
                state,
                state.nodes.keys().copied().collect(),
            )
        })
    }
}

pub struct GlobalInvariantMonitor {
    pub invariants: Vec<Box<dyn Invariant>>,
}

impl GlobalInvariantMonitor {
    pub fn new_default() -> Self {
        Self {
            invariants: vec![
                Box::new(SafetyInvariant),
                Box::new(LivenessInvariant),
                Box::new(IdempotencyInvariant),
                Box::new(EscapeHatchInvariant),
                Box::new(ProverConsistencyInvariant),
                Box::new(FinalizationMonotonicityInvariant),
            ],
        }
    }

    pub fn with_custom_invariants(mut self, invariants: &[CustomInvariant]) -> Self {
        for invariant in invariants {
            self.invariants
                .push(Box::new(CustomInvariantChecker::new(invariant.clone())));
        }
        self
    }

    pub async fn check_all(&self, state: &SimulationState) -> Vec<InvariantViolation> {
        let mut violations = Vec::new();
        for invariant in &self.invariants {
            if let Some(violation) = invariant.check(state).await {
                violations.push(violation);
            }
        }
        violations
    }
}

fn new_violation(
    invariant_id: InvariantId,
    description: String,
    state: &SimulationState,
    involved_nodes: Vec<NodeId>,
) -> InvariantViolation {
    InvariantViolation {
        invariant_id,
        description,
        violated_at_tick: state.tick,
        involved_nodes,
        evidence: ViolationEvidence {
            seed: state.seed,
            event_trace: state.trace.clone(),
            node_states: state.nodes.clone(),
        },
    }
}

fn evaluate_custom_expr(state: &SimulationState, spec: &CustomInvariant) -> Option<String> {
    let expr = spec.check_expr.trim();
    if let Some(rhs) = expr.strip_prefix("ticks_since_last_batch <= ") {
        if let Ok(limit) = rhs.trim().parse::<u64>() {
            let observed = state
                .custom_metrics
                .get("ticks_since_last_batch")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            if observed > limit {
                return Some(format!(
                    "custom invariant `{}` violated: ticks_since_last_batch={} > {}",
                    spec.id, observed, limit
                ));
            }
        }
        return None;
    }

    if let Some(inner) = expr
        .strip_prefix("forced_withdrawal_reachable_within_ticks(")
        .and_then(|tail| tail.strip_suffix(')'))
    {
        if let Ok(limit) = inner.trim().parse::<u64>() {
            let blocked: Vec<NodeId> = state
                .nodes
                .iter()
                .filter_map(|(node_id, snapshot)| {
                    if !snapshot.forced_withdrawal_reachable
                        && snapshot.forced_withdrawal_blocked_ticks > limit
                    {
                        Some(*node_id)
                    } else {
                        None
                    }
                })
                .collect();
            if !blocked.is_empty() {
                return Some(format!(
                    "custom invariant `{}` violated: forced withdrawal blocked for {:?}",
                    spec.id, blocked
                ));
            }
        }
    }

    None
}
