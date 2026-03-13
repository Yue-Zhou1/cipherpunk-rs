use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::engine::SandboxImage;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub enum ToolFamily {
    Kani,
    Z3,
    CargoFuzz,
    MadSim,
    Chaos,
    CircomZ3,
    CairoExternal,
    LeanExternal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum ToolTarget {
    Symbol { id: String },
    File { path: String },
    Domain { id: String },
    Session,
}

impl ToolTarget {
    pub fn display_value(&self) -> &str {
        match self {
            ToolTarget::Symbol { id } => id,
            ToolTarget::File { path } => path,
            ToolTarget::Domain { id } => id,
            ToolTarget::Session => "session",
        }
    }

    pub fn slug(&self) -> String {
        self.display_value()
            .chars()
            .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ToolBudget {
    pub timeout_secs: u64,
    pub cpu_cores: f64,
    pub memory_mb: u64,
    pub disk_gb: u64,
    pub allow_network: bool,
}

impl Default for ToolBudget {
    fn default() -> Self {
        Self {
            timeout_secs: 180,
            cpu_cores: 2.0,
            memory_mb: 2048,
            disk_gb: 4,
            allow_network: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ToolActionRequest {
    pub session_id: String,
    pub workspace_root: Option<PathBuf>,
    pub tool_family: ToolFamily,
    pub target: ToolTarget,
    pub budget: ToolBudget,
}

impl ToolActionRequest {
    pub fn kani(session_id: impl Into<String>, symbol_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            workspace_root: None,
            tool_family: ToolFamily::Kani,
            target: ToolTarget::Symbol {
                id: symbol_id.into(),
            },
            budget: ToolBudget::default(),
        }
    }

    pub fn z3(session_id: impl Into<String>, symbol_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            workspace_root: None,
            tool_family: ToolFamily::Z3,
            target: ToolTarget::Symbol {
                id: symbol_id.into(),
            },
            budget: ToolBudget::default(),
        }
    }

    pub fn cargo_fuzz(session_id: impl Into<String>, symbol_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            workspace_root: None,
            tool_family: ToolFamily::CargoFuzz,
            target: ToolTarget::Symbol {
                id: symbol_id.into(),
            },
            budget: ToolBudget {
                timeout_secs: 900,
                cpu_cores: 2.0,
                memory_mb: 4096,
                disk_gb: 8,
                allow_network: false,
            },
        }
    }

    pub fn madsim(session_id: impl Into<String>, domain_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            workspace_root: None,
            tool_family: ToolFamily::MadSim,
            target: ToolTarget::Domain {
                id: domain_id.into(),
            },
            budget: ToolBudget {
                timeout_secs: 600,
                cpu_cores: 2.0,
                memory_mb: 4096,
                disk_gb: 4,
                allow_network: false,
            },
        }
    }

    pub fn chaos(session_id: impl Into<String>, domain_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            workspace_root: None,
            tool_family: ToolFamily::Chaos,
            target: ToolTarget::Domain {
                id: domain_id.into(),
            },
            budget: ToolBudget {
                timeout_secs: 600,
                cpu_cores: 2.0,
                memory_mb: 4096,
                disk_gb: 4,
                allow_network: false,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum ToolActionStatus {
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ToolActionResult {
    pub action_id: String,
    pub session_id: String,
    pub tool_family: ToolFamily,
    pub target: ToolTarget,
    pub command: Vec<String>,
    pub artifact_refs: Vec<String>,
    pub rationale: String,
    pub status: ToolActionStatus,
    pub stdout_preview: Option<String>,
    pub stderr_preview: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolExecutionPlan {
    pub tool_family: ToolFamily,
    pub image: SandboxImage,
    pub command: Vec<String>,
    pub artifact_refs: Vec<String>,
    pub rationale: String,
}
