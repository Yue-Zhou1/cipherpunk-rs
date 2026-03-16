use std::collections::HashSet;
use std::ops::RangeInclusive;
use std::path::PathBuf;

use anyhow::{Context, Result};
use rand::rngs::StdRng;
use rand::{RngCore, SeedableRng};
use serde::{Deserialize, Serialize};
use serde_yaml::Value as YamlValue;

pub type NodeId = u32;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChaosScript {
    pub name: String,
    pub description: String,
    pub steps: Vec<ChaosStep>,
}

impl ChaosScript {
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self).context("serialize chaos script to json")
    }

    pub fn from_json(value: &str) -> Result<Self> {
        serde_json::from_str(value).context("deserialize chaos script from json")
    }

    pub fn from_yaml(value: &str) -> Result<Self> {
        let root: YamlValue =
            serde_yaml::from_str(value).context("deserialize chaos script from yaml value")?;
        let mapping = root
            .as_mapping()
            .context("chaos script yaml root must be a mapping")?;

        let name = get_yaml_str(mapping, "name")?.to_string();
        let description = get_yaml_str(mapping, "description")?.to_string();
        let steps_value = mapping
            .get(YamlValue::String("steps".to_string()))
            .context("chaos script yaml missing `steps`")?;
        let step_list = steps_value
            .as_sequence()
            .context("chaos script `steps` must be a sequence")?;

        let mut steps = Vec::with_capacity(step_list.len());
        for step in step_list {
            steps.push(parse_step(step)?);
        }

        Ok(Self {
            name,
            description,
            steps,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ChaosStep {
    Partition {
        nodes: Vec<NodeId>,
        duration_ticks: u64,
    },
    Delay {
        nodes: Vec<NodeId>,
        delay_ms: u64,
        jitter_ms: u64,
    },
    Drop {
        nodes: Vec<NodeId>,
        #[serde(with = "f64_serde")]
        drop_rate: f64,
    },
    Duplicate {
        nodes: Vec<NodeId>,
        #[serde(with = "f64_serde")]
        dup_rate: f64,
    },
    Eclipse {
        target: NodeId,
        duration_ticks: u64,
    },
    DoubleVote {
        node: NodeId,
        at_height: u64,
    },
    SelectiveForward {
        node: NodeId,
        drop_from: Vec<NodeId>,
    },
    ForgeVrfOutput {
        node: NodeId,
    },
    RefuseSync {
        node: NodeId,
        for_heights: RangeInclusive<u64>,
    },
    SequencerDropTx {
        sequencer: NodeId,
        tx_pattern: TxPattern,
    },
    ProposerReplayBatch {
        proposer: NodeId,
        batch_index: u64,
    },
    ProverSubmitWrongStateRoot {
        prover: NodeId,
        at_height: u64,
    },
    Wait {
        ticks: u64,
    },
    CheckInvariant {
        invariant: InvariantId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TxPattern {
    All,
    Prefix(String),
    Exact(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvariantId {
    Safety,
    Liveness,
    #[serde(rename = "liveness_except_nodes")]
    LivenessExceptNodes(Vec<NodeId>),
    Idempotency,
    EscapeHatch,
    ProverConsistency,
    FinalizationMonotonicity,
    Custom(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntheticConsensusFixture {
    safety_broken: bool,
    liveness_broken: bool,
}

impl SyntheticConsensusFixture {
    pub fn healthy() -> Self {
        Self {
            safety_broken: false,
            liveness_broken: false,
        }
    }

    pub fn broken_safety() -> Self {
        Self {
            safety_broken: true,
            liveness_broken: false,
        }
    }

    pub fn broken_safety_and_liveness() -> Self {
        Self {
            safety_broken: true,
            liveness_broken: true,
        }
    }

    pub fn run_with_seed(&self, seed: u64, script: &ChaosScript) -> BridgeScenarioResult {
        ScenarioRunner::new(seed, self.clone()).run(script)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceEvent {
    pub tick: u64,
    pub step_index: usize,
    pub kind: String,
    pub rng_nonce: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvariantViolationRecord {
    pub invariant: InvariantId,
    pub tick: u64,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BridgeScenarioResult {
    pub trace: Vec<TraceEvent>,
    pub invariant_violations: Vec<InvariantViolationRecord>,
}

impl BridgeScenarioResult {
    pub fn has_safety_violation(&self) -> bool {
        self.invariant_violations
            .iter()
            .any(|violation| violation.invariant == InvariantId::Safety)
    }

    pub fn has_liveness_violation(&self) -> bool {
        self.invariant_violations
            .iter()
            .any(|violation| violation.invariant == InvariantId::Liveness)
    }

    pub fn max_tick(&self) -> u64 {
        self.trace.last().map(|event| event.tick).unwrap_or(0)
    }
}

pub type ScenarioOutcome = BridgeScenarioResult;

pub struct ScenarioRunner {
    seed: u64,
    fixture: SyntheticConsensusFixture,
}

impl ScenarioRunner {
    pub fn new(seed: u64, fixture: SyntheticConsensusFixture) -> Self {
        Self { seed, fixture }
    }

    pub fn run(&self, script: &ChaosScript) -> ScenarioOutcome {
        let mut rng = StdRng::seed_from_u64(self.seed);
        let mut trace = Vec::new();
        let mut invariant_violations = Vec::new();
        let mut tick = 0u64;
        let mut partition_active = false;
        let mut double_vote_seen = false;
        let mut isolated_nodes = HashSet::<NodeId>::new();

        for (step_index, step) in script.steps.iter().enumerate() {
            let (kind, tick_advance) = match step {
                ChaosStep::Partition { duration_ticks, .. } => {
                    partition_active = true;
                    if let ChaosStep::Partition { nodes, .. } = step {
                        isolated_nodes.extend(nodes.iter().copied());
                    }
                    ("partition".to_string(), *duration_ticks)
                }
                ChaosStep::Delay { delay_ms, .. } => ("delay".to_string(), *delay_ms),
                ChaosStep::Drop { .. } => ("drop".to_string(), 1),
                ChaosStep::Duplicate { .. } => ("duplicate".to_string(), 1),
                ChaosStep::Eclipse { duration_ticks, .. } => {
                    partition_active = true;
                    if let ChaosStep::Eclipse { target, .. } = step {
                        isolated_nodes.insert(*target);
                    }
                    ("eclipse".to_string(), *duration_ticks)
                }
                ChaosStep::DoubleVote { at_height, .. } => {
                    double_vote_seen = true;
                    ("double_vote".to_string(), *at_height)
                }
                ChaosStep::SelectiveForward { .. } => ("selective_forward".to_string(), 1),
                ChaosStep::ForgeVrfOutput { .. } => ("forge_vrf_output".to_string(), 1),
                ChaosStep::RefuseSync { for_heights, .. } => (
                    "refuse_sync".to_string(),
                    for_heights.end() - for_heights.start(),
                ),
                ChaosStep::SequencerDropTx { .. } => ("sequencer_drop_tx".to_string(), 1),
                ChaosStep::ProposerReplayBatch { .. } => ("proposer_replay_batch".to_string(), 1),
                ChaosStep::ProverSubmitWrongStateRoot { .. } => {
                    ("prover_submit_wrong_state_root".to_string(), 1)
                }
                ChaosStep::Wait { ticks } => ("wait".to_string(), *ticks),
                ChaosStep::CheckInvariant { invariant } => {
                    match invariant {
                        InvariantId::Safety
                            if self.fixture.safety_broken
                                && (partition_active || double_vote_seen) =>
                        {
                            invariant_violations.push(InvariantViolationRecord {
                                invariant: InvariantId::Safety,
                                tick,
                                description:
                                    "safety violated: conflicting commits under adversarial scenario"
                                        .to_string(),
                            });
                        }
                        InvariantId::Liveness
                            if self.fixture.liveness_broken && partition_active =>
                        {
                            invariant_violations.push(InvariantViolationRecord {
                                invariant: InvariantId::Liveness,
                                tick,
                                description:
                                    "liveness violated: isolated partition cannot make progress"
                                        .to_string(),
                            });
                        }
                        InvariantId::LivenessExceptNodes(exempt)
                            if self.fixture.liveness_broken && partition_active =>
                        {
                            let exempt_set: HashSet<NodeId> = exempt.iter().copied().collect();
                            let non_exempt_isolated: Vec<NodeId> = isolated_nodes
                                .iter()
                                .filter(|node| !exempt_set.contains(node))
                                .copied()
                                .collect();
                            if !non_exempt_isolated.is_empty() {
                                invariant_violations.push(InvariantViolationRecord {
                                    invariant: InvariantId::LivenessExceptNodes(exempt.clone()),
                                    tick,
                                    description: format!(
                                        "liveness violated outside exempt nodes: {non_exempt_isolated:?}"
                                    ),
                                });
                            }
                        }
                        _ => {}
                    }
                    ("check_invariant".to_string(), 0)
                }
            };

            trace.push(TraceEvent {
                tick,
                step_index,
                kind,
                rng_nonce: rng.next_u64(),
            });

            tick = tick.saturating_add(tick_advance);
        }

        BridgeScenarioResult {
            trace,
            invariant_violations,
        }
    }
}

pub fn builtin_scenarios_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scenarios")
}

pub fn load_builtin_scenario(file_name: &str) -> Result<ChaosScript> {
    let path = builtin_scenarios_dir().join(file_name);
    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read builtin scenario {}", path.display()))?;
    ChaosScript::from_yaml(&raw)
}

fn parse_step(value: &YamlValue) -> Result<ChaosStep> {
    if let Ok(step) = serde_yaml::from_value::<ChaosStep>(value.clone()) {
        return Ok(step);
    }

    let mapping = value
        .as_mapping()
        .context("step must be a mapping or tagged enum")?;
    if mapping.len() != 1 {
        anyhow::bail!("step mapping must contain exactly one variant key");
    }
    let (key, payload) = mapping
        .iter()
        .next()
        .context("step mapping missing variant key")?;
    let variant = key.as_str().context("step variant key must be a string")?;

    match variant {
        "Partition" => {
            #[derive(Deserialize)]
            struct Payload {
                nodes: Vec<NodeId>,
                duration_ticks: u64,
            }
            let p: Payload =
                serde_yaml::from_value(payload.clone()).context("invalid Partition payload")?;
            Ok(ChaosStep::Partition {
                nodes: p.nodes,
                duration_ticks: p.duration_ticks,
            })
        }
        "Delay" => {
            #[derive(Deserialize)]
            struct Payload {
                nodes: Vec<NodeId>,
                delay_ms: u64,
                jitter_ms: u64,
            }
            let p: Payload =
                serde_yaml::from_value(payload.clone()).context("invalid Delay payload")?;
            Ok(ChaosStep::Delay {
                nodes: p.nodes,
                delay_ms: p.delay_ms,
                jitter_ms: p.jitter_ms,
            })
        }
        "Drop" => {
            #[derive(Deserialize)]
            struct Payload {
                nodes: Vec<NodeId>,
                drop_rate: f64,
            }
            let p: Payload =
                serde_yaml::from_value(payload.clone()).context("invalid Drop payload")?;
            Ok(ChaosStep::Drop {
                nodes: p.nodes,
                drop_rate: p.drop_rate,
            })
        }
        "Duplicate" => {
            #[derive(Deserialize)]
            struct Payload {
                nodes: Vec<NodeId>,
                dup_rate: f64,
            }
            let p: Payload =
                serde_yaml::from_value(payload.clone()).context("invalid Duplicate payload")?;
            Ok(ChaosStep::Duplicate {
                nodes: p.nodes,
                dup_rate: p.dup_rate,
            })
        }
        "Eclipse" => {
            #[derive(Deserialize)]
            struct Payload {
                target: NodeId,
                duration_ticks: u64,
            }
            let p: Payload =
                serde_yaml::from_value(payload.clone()).context("invalid Eclipse payload")?;
            Ok(ChaosStep::Eclipse {
                target: p.target,
                duration_ticks: p.duration_ticks,
            })
        }
        "DoubleVote" => {
            #[derive(Deserialize)]
            struct Payload {
                node: NodeId,
                at_height: u64,
            }
            let p: Payload =
                serde_yaml::from_value(payload.clone()).context("invalid DoubleVote payload")?;
            Ok(ChaosStep::DoubleVote {
                node: p.node,
                at_height: p.at_height,
            })
        }
        "SelectiveForward" => {
            #[derive(Deserialize)]
            struct Payload {
                node: NodeId,
                drop_from: Vec<NodeId>,
            }
            let p: Payload = serde_yaml::from_value(payload.clone())
                .context("invalid SelectiveForward payload")?;
            Ok(ChaosStep::SelectiveForward {
                node: p.node,
                drop_from: p.drop_from,
            })
        }
        "ForgeVrfOutput" => {
            #[derive(Deserialize)]
            struct Payload {
                node: NodeId,
            }
            let p: Payload = serde_yaml::from_value(payload.clone())
                .context("invalid ForgeVrfOutput payload")?;
            Ok(ChaosStep::ForgeVrfOutput { node: p.node })
        }
        "RefuseSync" => {
            #[derive(Deserialize)]
            struct Payload {
                node: NodeId,
                for_heights: RangeInclusive<u64>,
            }
            let p: Payload =
                serde_yaml::from_value(payload.clone()).context("invalid RefuseSync payload")?;
            Ok(ChaosStep::RefuseSync {
                node: p.node,
                for_heights: p.for_heights,
            })
        }
        "SequencerDropTx" => {
            #[derive(Deserialize)]
            struct Payload {
                sequencer: NodeId,
                tx_pattern: TxPattern,
            }
            let p: Payload = serde_yaml::from_value(payload.clone())
                .context("invalid SequencerDropTx payload")?;
            Ok(ChaosStep::SequencerDropTx {
                sequencer: p.sequencer,
                tx_pattern: p.tx_pattern,
            })
        }
        "ProposerReplayBatch" => {
            #[derive(Deserialize)]
            struct Payload {
                proposer: NodeId,
                batch_index: u64,
            }
            let p: Payload = serde_yaml::from_value(payload.clone())
                .context("invalid ProposerReplayBatch payload")?;
            Ok(ChaosStep::ProposerReplayBatch {
                proposer: p.proposer,
                batch_index: p.batch_index,
            })
        }
        "ProverSubmitWrongStateRoot" => {
            #[derive(Deserialize)]
            struct Payload {
                prover: NodeId,
                at_height: u64,
            }
            let p: Payload = serde_yaml::from_value(payload.clone())
                .context("invalid ProverSubmitWrongStateRoot payload")?;
            Ok(ChaosStep::ProverSubmitWrongStateRoot {
                prover: p.prover,
                at_height: p.at_height,
            })
        }
        "Wait" => {
            #[derive(Deserialize)]
            struct Payload {
                ticks: u64,
            }
            let p: Payload =
                serde_yaml::from_value(payload.clone()).context("invalid Wait payload")?;
            Ok(ChaosStep::Wait { ticks: p.ticks })
        }
        "CheckInvariant" => {
            let invariant =
                parse_invariant_id(payload).context("invalid CheckInvariant payload")?;
            Ok(ChaosStep::CheckInvariant { invariant })
        }
        _ => anyhow::bail!("unsupported chaos step variant `{variant}`"),
    }
}

fn get_yaml_str<'a>(mapping: &'a serde_yaml::Mapping, key: &str) -> Result<&'a str> {
    mapping
        .get(YamlValue::String(key.to_string()))
        .and_then(YamlValue::as_str)
        .with_context(|| format!("missing or invalid `{key}` in chaos script yaml"))
}

fn parse_invariant_id(value: &YamlValue) -> Result<InvariantId> {
    if let Some(mapping) = value.as_mapping()
        && mapping.len() == 1
    {
        let (key, payload) = mapping
            .iter()
            .next()
            .context("invariant map must contain exactly one entry")?;
        let key = key.as_str().context("invariant map key must be a string")?;
        return match key {
            "liveness_except_nodes" => {
                let nodes: Vec<NodeId> =
                    serde_yaml::from_value(payload.clone()).context("invalid node list")?;
                Ok(InvariantId::LivenessExceptNodes(nodes))
            }
            "custom" => {
                let id: String =
                    serde_yaml::from_value(payload.clone()).context("invalid custom id")?;
                Ok(InvariantId::Custom(id))
            }
            _ => anyhow::bail!("unsupported invariant map key `{key}`"),
        };
    }

    serde_yaml::from_value(value.clone()).context("invalid invariant id")
}

mod f64_serde {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(value: &f64, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_f64(*value)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<f64, D::Error>
    where
        D: Deserializer<'de>,
    {
        f64::deserialize(deserializer)
    }
}
