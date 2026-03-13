use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;

use crate::LlmProvider;
use crate::audit_config::AuditConfig;
use crate::finding::Finding;
use crate::workspace::CargoWorkspace;

#[async_trait]
pub trait AuditEngine: Send + Sync {
    fn name(&self) -> &str;
    async fn analyze(&self, ctx: &AuditContext) -> Result<Vec<Finding>>;
    async fn supports(&self, ctx: &AuditContext) -> bool;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SandboxImage {
    Kani,
    Z3,
    Miri,
    MadSim,
    Fuzz,
    Custom(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandboxMount {
    pub host_path: PathBuf,
    pub container_path: PathBuf,
    pub read_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SandboxNetworkPolicy {
    Disabled,
    Allowlist(Vec<String>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct SandboxBudget {
    pub cpu_cores: f64,
    pub memory_mb: u64,
    pub disk_gb: u64,
    pub timeout_secs: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SandboxRequest {
    pub image: SandboxImage,
    pub command: Vec<String>,
    pub mounts: Vec<SandboxMount>,
    pub env: HashMap<String, String>,
    pub budget: SandboxBudget,
    pub network: SandboxNetworkPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SandboxResourceUsage {
    pub memory_bytes: Option<u64>,
    pub cpu_nanos: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandboxResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub artifacts: Vec<PathBuf>,
    pub container_digest: String,
    pub duration_ms: u64,
    pub resource_usage: SandboxResourceUsage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceArtifact {
    pub artifact_id: String,
    pub session_id: Option<String>,
    pub relative_path: PathBuf,
    pub bytes: Vec<u8>,
}

#[async_trait]
pub trait SandboxRunner: Send + Sync {
    async fn execute(&self, request: SandboxRequest) -> Result<SandboxResult>;
}

#[async_trait]
pub trait EvidenceWriter: Send + Sync {
    async fn save(&self, artifact: EvidenceArtifact) -> Result<()>;
}

#[derive(Debug, Default, Clone)]
pub struct NoopSandboxRunner;

#[async_trait]
impl SandboxRunner for NoopSandboxRunner {
    async fn execute(&self, _request: SandboxRequest) -> Result<SandboxResult> {
        Ok(SandboxResult {
            exit_code: 0,
            stdout: String::new(),
            stderr: String::new(),
            artifacts: vec![],
            container_digest: "noop".to_string(),
            duration_ms: 0,
            resource_usage: SandboxResourceUsage::default(),
        })
    }
}

#[derive(Debug, Default, Clone)]
pub struct NoopEvidenceWriter;

#[async_trait]
impl EvidenceWriter for NoopEvidenceWriter {
    async fn save(&self, _artifact: EvidenceArtifact) -> Result<()> {
        Ok(())
    }
}

pub struct AuditContext {
    pub config: Arc<AuditConfig>,
    pub workspace: Arc<CargoWorkspace>,
    pub sandbox: Arc<dyn SandboxRunner>,
    pub evidence_store: Arc<dyn EvidenceWriter>,
    pub llm: Option<Arc<dyn LlmProvider>>,
}
