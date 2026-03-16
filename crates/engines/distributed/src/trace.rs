use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::chaos::{NodeId, TraceEvent};
use crate::util::sanitize_ident;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    MessageSent,
    MessageReceived,
    NodeCrash,
    InvariantCheck,
    ChaosStep,
    Custom(String),
}

impl EventKind {
    fn as_slug(&self) -> String {
        match self {
            Self::MessageSent => "message_sent".to_string(),
            Self::MessageReceived => "message_received".to_string(),
            Self::NodeCrash => "node_crash".to_string(),
            Self::InvariantCheck => "invariant_check".to_string(),
            Self::ChaosStep => "chaos_step".to_string(),
            Self::Custom(kind) => sanitize_ident(kind),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimEvent {
    pub tick: u64,
    pub kind: EventKind,
    pub node: NodeId,
    pub payload: Value,
}

impl From<TraceEvent> for SimEvent {
    fn from(value: TraceEvent) -> Self {
        let kind = match value.kind.as_str() {
            "message_sent" => EventKind::MessageSent,
            "message_received" => EventKind::MessageReceived,
            "node_crash" => EventKind::NodeCrash,
            "check_invariant" => EventKind::InvariantCheck,
            _ => EventKind::ChaosStep,
        };

        Self {
            tick: value.tick,
            kind,
            node: 0,
            payload: json!({
                "step_index": value.step_index,
                "rng_nonce": value.rng_nonce,
                "kind": value.kind,
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceCapture {
    pub seed: u64,
    pub events: Vec<SimEvent>,
    pub duration_ticks: u64,
}

impl TraceCapture {
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|error| {
            format!("{{\"error\":\"trace serialization failed: {error}\"}}")
        })
    }

    pub fn to_replay_script(&self, harness_path: &Path, container_image: &str) -> String {
        format!(
            r#"#!/usr/bin/env bash
set -euo pipefail

CONTAINER_IMAGE="{container_image}"
HARNESS_PATH="{harness_path}"
SEED="{seed}"

docker run --rm \
  -v "$PWD:/workspace" \
  "$CONTAINER_IMAGE" \
  bash -lc "cd /workspace && cargo run --manifest-path $HARNESS_PATH/Cargo.toml -- --seed {seed}"
"#,
            container_image = container_image,
            harness_path = harness_path.display(),
            seed = self.seed
        )
    }

    pub fn shrink(&self, violation_tick: u64) -> TraceCapture {
        let mut relevant: Vec<SimEvent> = self
            .events
            .iter()
            .filter(|event| event.tick <= violation_tick)
            .cloned()
            .collect();

        if relevant.is_empty() {
            relevant = self.events.clone();
        }

        if relevant.len() >= 50 {
            let mut compact = Vec::new();
            compact.push(relevant[0].clone());

            let tail_len = 48usize;
            let start = relevant.len().saturating_sub(tail_len);
            compact.extend(relevant[start..].iter().cloned());
            relevant = compact;
        }

        TraceCapture {
            seed: self.seed,
            duration_ticks: relevant.last().map(|event| event.tick).unwrap_or(0),
            events: relevant,
        }
    }

    pub fn to_regression_test(&self, test_name: &str) -> String {
        let safe_name = sanitize_ident(test_name);
        let event_rows = self
            .events
            .iter()
            .map(|event| {
                format!(
                    "        ({}, \"{}\", {}),\n",
                    event.tick,
                    event.kind.as_slug(),
                    event.node
                )
            })
            .collect::<String>();

        format!(
            r#"#[test]
fn {test_name}() {{
    let seed: u64 = {seed};
    let events: &[(u64, &str, u32)] = &[
{event_rows}    ];

    assert!(!events.is_empty(), "regression trace should include events");
    assert_eq!(seed, {seed});
}}
"#,
            test_name = safe_name,
            seed = self.seed,
            event_rows = event_rows
        )
    }

    pub fn write_evidence_files(
        &self,
        evidence_pack_root: &Path,
        finding_id: &str,
        harness_path: &Path,
        container_image: &str,
    ) -> Result<PathBuf> {
        let traces_dir = evidence_pack_root.join(finding_id).join("traces");
        std::fs::create_dir_all(&traces_dir).with_context(|| {
            format!("failed to create traces directory {}", traces_dir.display())
        })?;

        let trace_json_path = traces_dir.join("trace.json");
        let seed_path = traces_dir.join("seed.txt");
        let replay_path = traces_dir.join("replay.sh");

        std::fs::write(&trace_json_path, self.to_json())
            .with_context(|| format!("failed to write {}", trace_json_path.display()))?;
        std::fs::write(&seed_path, self.seed.to_string())
            .with_context(|| format!("failed to write {}", seed_path.display()))?;
        std::fs::write(
            &replay_path,
            self.to_replay_script(harness_path, container_image),
        )
        .with_context(|| format!("failed to write {}", replay_path.display()))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o755);
            std::fs::set_permissions(&replay_path, perms)
                .with_context(|| format!("failed to chmod {}", replay_path.display()))?;
        }

        Ok(traces_dir)
    }

    pub fn write_regression_test(&self, test_name: &str, output_dir: &Path) -> Result<PathBuf> {
        std::fs::create_dir_all(output_dir)
            .with_context(|| format!("failed to create {}", output_dir.display()))?;

        let file_name = format!("{}.rs", sanitize_ident(test_name));
        let test_path = output_dir.join(file_name);
        std::fs::write(&test_path, self.to_regression_test(test_name))
            .with_context(|| format!("failed to write {}", test_path.display()))?;
        Ok(test_path)
    }
}
