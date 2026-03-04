use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;

use crate::audit_config::AuditConfig;
use crate::finding::Finding;
use crate::workspace::CargoWorkspace;
use crate::{EvidenceStore, LlmProvider, SandboxExecutor};

#[async_trait]
pub trait AuditEngine: Send + Sync {
    fn name(&self) -> &str;
    async fn analyze(&self, ctx: &AuditContext) -> Result<Vec<Finding>>;
    async fn supports(&self, ctx: &AuditContext) -> bool;
}

pub struct AuditContext {
    pub config: Arc<AuditConfig>,
    pub workspace: Arc<CargoWorkspace>,
    pub sandbox: Arc<SandboxExecutor>,
    pub evidence_store: Arc<EvidenceStore>,
    pub llm: Option<Arc<dyn LlmProvider>>,
}
