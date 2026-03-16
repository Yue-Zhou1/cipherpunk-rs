use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerExecutionRequest {
    pub request_id: String,
    pub image: String,
    pub command: Vec<String>,
    pub mounts: Vec<WorkerMount>,
    pub env: HashMap<String, String>,
    pub budget: WorkerResourceBudget,
    pub network: WorkerNetworkPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerMount {
    pub host_path: PathBuf,
    pub container_path: PathBuf,
    pub read_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerResourceBudget {
    pub cpu_millicores: u32,
    pub memory_mb: u64,
    pub disk_gb: u64,
    pub timeout_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerNetworkPolicy {
    Disabled,
    Allowlist(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerArtifactRef {
    pub path: PathBuf,
    pub digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedArtifactManifest {
    pub manifest_id: String,
    pub signature: String,
    pub artifacts: Vec<WorkerArtifactRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerResourceUsage {
    pub memory_bytes: Option<u64>,
    pub cpu_nanos: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerExecutionResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub artifacts: Vec<PathBuf>,
    pub container_digest: String,
    pub duration_ms: u64,
    pub resource_usage: WorkerResourceUsage,
    pub signed_manifest: SignedArtifactManifest,
}
