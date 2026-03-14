use std::collections::HashMap;
use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct FindingId(String);

impl FindingId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

impl std::fmt::Display for FindingId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Finding {
    pub id: FindingId,
    pub title: String,
    pub severity: Severity,
    pub category: FindingCategory,
    pub framework: Framework,
    pub affected_components: Vec<CodeLocation>,
    pub prerequisites: String,
    pub exploit_path: String,
    pub impact: String,
    pub evidence: Evidence,
    pub evidence_gate_level: u8,
    pub llm_generated: bool,
    pub recommendation: String,
    pub regression_test: Option<String>,
    pub status: FindingStatus,
    pub regression_check: bool,
    pub verification_status: VerificationStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum VerificationStatus {
    Verified,
    Unverified { reason: String },
}

impl VerificationStatus {
    pub fn unverified(reason: impl Into<String>) -> Self {
        Self::Unverified {
            reason: reason.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Evidence {
    pub command: Option<String>,
    pub seed: Option<String>,
    pub trace_file: Option<PathBuf>,
    pub counterexample: Option<String>,
    pub harness_path: Option<PathBuf>,
    pub smt2_file: Option<PathBuf>,
    pub container_digest: String,
    pub tool_versions: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CodeLocation {
    pub crate_name: String,
    pub module: String,
    pub file: PathBuf,
    pub line_range: (u32, u32),
    pub snippet: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum Severity {
    Critical,
    High,
    Medium,
    Low,
    Observation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum FindingCategory {
    UnderConstrained,
    SpecMismatch,
    CryptoMisuse,
    Replay,
    DoS,
    Race,
    Incentive,
    UnsafeUB,
    SideChannel,
    SupplyChain,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub enum Framework {
    Halo2,
    Circom,
    Cairo,
    SP1,
    RISC0,
    MadSim,
    Loom,
    Static,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum FindingStatus {
    Open,
    Acknowledged,
    Fixed,
    Regressed,
    WontFix,
}
