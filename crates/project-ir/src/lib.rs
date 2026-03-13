//! Shared project IR for workstation graph lenses.
//!
//! Note: `Graph` is modeled as `Graph<Node, Edge>` instead of `Graph<Node>`.
//! The explicit edge type keeps lens-specific edge payloads strongly typed
//! (for example, `DataflowEdge` with redaction metadata).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use audit_agent_core::workspace::{CargoWorkspace, CrateKind, CrateMeta, DependencyGraph};

mod cairo;
mod circom;
pub mod graph;
pub mod redaction;
mod rust;
pub mod semantic;

use cairo::CairoMapper;
use circom::CircomMapper;
use rust::RustMapper;

pub use graph::{
    BasicEdge, DataflowEdge, DataflowNode, FeatureNode, FileNode, FrameworkView, Graph, ProjectIr,
    ProjectIrFragment, SymbolNode,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphLensKind {
    File,
    Symbol,
    Feature,
    Dataflow,
    Framework,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SecurityOverview {
    pub assets: Vec<String>,
    pub trust_boundaries: Vec<String>,
    pub hotspots: Vec<String>,
    pub review_notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChecklistDomainPlan {
    pub id: String,
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ChecklistPlan {
    pub domains: Vec<ChecklistDomainPlan>,
}

pub trait LanguageMapper {
    fn can_handle(&self, workspace: &CargoWorkspace) -> bool;
    fn build(&self, workspace: &CargoWorkspace) -> Result<ProjectIrFragment>;
}

#[derive(Debug, Clone)]
pub struct ProjectIrBuilder {
    root: PathBuf,
    allow_value_previews: bool,
}

impl ProjectIrBuilder {
    pub fn for_path(path: impl AsRef<Path>) -> Self {
        Self {
            root: path.as_ref().to_path_buf(),
            allow_value_previews: false,
        }
    }

    pub fn with_value_previews(mut self, enabled: bool) -> Self {
        self.allow_value_previews = enabled;
        self
    }

    pub async fn build(self) -> Result<ProjectIr> {
        let workspace = workspace_from_path(&self.root)?;
        let mut ir = ProjectIr::default();
        let mappers: Vec<Box<dyn LanguageMapper>> = vec![
            Box::new(RustMapper),
            Box::new(CircomMapper),
            Box::new(CairoMapper),
        ];

        for mapper in mappers {
            if mapper.can_handle(&workspace) {
                let fragment = mapper.build(&workspace)?;
                ir.absorb(fragment);
            }
        }

        redaction::redact_dataflow(&mut ir.dataflow_graph.edges, self.allow_value_previews);
        Ok(ir)
    }
}

impl ProjectIr {
    pub fn security_overview(&self) -> SecurityOverview {
        let mut assets = self
            .file_graph
            .nodes
            .iter()
            .take(8)
            .map(|node| node.path.display().to_string())
            .collect::<Vec<_>>();
        if assets.is_empty() {
            assets.push("No indexed assets yet".to_string());
        }

        let mut trust_boundaries = Vec::<String>::new();
        if !self.dataflow_graph.edges.is_empty() {
            trust_boundaries
                .push("Source inputs crossing into execution and persistence layers".to_string());
        }
        if self
            .file_graph
            .nodes
            .iter()
            .any(|node| node.path.to_string_lossy().contains("src-tauri"))
        {
            trust_boundaries.push("Desktop shell IPC boundary (frontend <-> backend)".to_string());
        }
        for view in &self.framework_views {
            trust_boundaries.push(format!(
                "{} analysis boundary with deterministic tool validation",
                view.framework
            ));
        }
        if trust_boundaries.is_empty() {
            trust_boundaries
                .push("Human verification boundary for AI-generated outputs".to_string());
        }
        trust_boundaries.sort();
        trust_boundaries.dedup();

        let mut hotspots = self
            .dataflow_graph
            .edges
            .iter()
            .take(10)
            .map(|edge| format!("{}: {} -> {}", edge.relation, edge.from, edge.to))
            .collect::<Vec<_>>();
        if hotspots.is_empty() {
            hotspots = self
                .feature_graph
                .nodes
                .iter()
                .take(10)
                .map(|node| format!("feature node: {}", node.name))
                .collect();
        }
        if hotspots.is_empty() {
            hotspots.push("No hotspots generated yet".to_string());
        }

        let previews_redacted = self
            .dataflow_graph
            .edges
            .iter()
            .any(|edge| edge.value_preview.is_none());
        let mut review_notes = vec![
            "AI-generated overview material must remain unverified until analyst review"
                .to_string(),
        ];
        if previews_redacted {
            review_notes.push("Dataflow value previews are redacted by default".to_string());
        } else {
            review_notes
                .push("Dataflow previews are visible by explicit policy approval".to_string());
        }

        SecurityOverview {
            assets,
            trust_boundaries,
            hotspots,
            review_notes,
        }
    }

    pub fn checklist_plan(&self) -> ChecklistPlan {
        let mut domains = Vec::<ChecklistDomainPlan>::new();
        let mut seen = HashSet::<String>::new();

        let has_rust = self
            .file_graph
            .nodes
            .iter()
            .any(|node| node.language == "rust");
        let has_crypto_indicators = self
            .file_graph
            .nodes
            .iter()
            .filter(|node| node.language == "rust")
            .any(|node| {
                contains_crypto_indicator(&node.path.to_string_lossy().to_ascii_lowercase())
            })
            || self
                .feature_graph
                .nodes
                .iter()
                .any(|node| contains_crypto_indicator(&node.name.to_ascii_lowercase()))
            || self
                .dataflow_graph
                .edges
                .iter()
                .any(|edge| contains_crypto_indicator(&edge.relation.to_ascii_lowercase()));
        if has_rust && has_crypto_indicators && seen.insert("crypto".to_string()) {
            domains.push(ChecklistDomainPlan {
                id: "crypto".to_string(),
                rationale:
                    "Rust modules and call/dataflow edges indicate cryptographic review paths"
                        .to_string(),
            });
        }

        let has_zk = self
            .file_graph
            .nodes
            .iter()
            .any(|node| node.language == "circom" || node.language == "cairo");
        if has_zk && seen.insert("zk".to_string()) {
            domains.push(ChecklistDomainPlan {
                id: "zk".to_string(),
                rationale: "Detected circuit or prover-oriented source files".to_string(),
            });
        }

        let has_network = self.file_graph.nodes.iter().any(|node| {
            let path = node.path.to_string_lossy().to_ascii_lowercase();
            path.contains("network") || path.contains("consensus") || path.contains("p2p")
        });
        if has_network && seen.insert("p2p-consensus".to_string()) {
            domains.push(ChecklistDomainPlan {
                id: "p2p-consensus".to_string(),
                rationale: "Repository paths indicate distributed protocol concerns".to_string(),
            });
        }

        if domains.is_empty() {
            domains.push(ChecklistDomainPlan {
                id: "core-audit".to_string(),
                rationale: "Fallback baseline checklist for unresolved project shape".to_string(),
            });
        }

        ChecklistPlan { domains }
    }
}

fn contains_crypto_indicator(value: &str) -> bool {
    const CRYPTO_INDICATORS: [&str; 20] = [
        "/crypto",
        "signature",
        "signer",
        "verify",
        "cipher",
        "hash",
        "keccak",
        "sha",
        "hmac",
        "kdf",
        "nonce",
        "merkle",
        "curve",
        "field",
        "scalar",
        "bls",
        "ecdsa",
        "ed25519",
        "schnorr",
        "secp",
    ];

    CRYPTO_INDICATORS
        .iter()
        .any(|indicator| value.contains(indicator))
}

fn workspace_from_path(root: &Path) -> Result<CargoWorkspace> {
    let absolute_root = if root.is_absolute() {
        root.to_path_buf()
    } else {
        let cwd_candidate = std::env::current_dir()
            .context("resolve current working directory")?
            .join(root);
        if cwd_candidate.exists() {
            cwd_candidate
        } else {
            let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .and_then(|value| value.parent())
                .context("resolve repository root from crate manifest")?
                .to_path_buf();
            repo_root.join(root)
        }
    };

    let crate_name = absolute_root
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| "workspace".to_string());

    Ok(CargoWorkspace {
        root: absolute_root.clone(),
        members: vec![CrateMeta {
            name: crate_name,
            path: absolute_root,
            kind: CrateKind::Lib,
            dependencies: vec![],
        }],
        dependency_graph: DependencyGraph {
            edges: HashMap::new(),
        },
        feature_flags: HashMap::new(),
    })
}
