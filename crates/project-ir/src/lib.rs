//! Shared project IR for workstation graph lenses.
//!
//! Note: `Graph` is modeled as `Graph<Node, Edge>` instead of `Graph<Node>`.
//! The explicit edge type keeps lens-specific edge payloads strongly typed
//! (for example, `DataflowEdge` with redaction metadata).

use std::collections::HashMap;
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
