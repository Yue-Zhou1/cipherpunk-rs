//! Shared project IR for workstation graph lenses.
//!
//! Note: `Graph` is modeled as `Graph<Node, Edge>` instead of `Graph<Node>`.
//! The explicit edge type keeps lens-specific edge payloads strongly typed
//! (for example, `DataflowEdge` with redaction metadata).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use audit_agent_core::workspace::{CargoWorkspace, CrateKind, CrateMeta, DependencyGraph};
use intake::workspace::WorkspaceAnalyzer;

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
    BasicEdge, ContextSnippet, DataflowEdge, DataflowNode, FeatureNode, FileNode, FrameworkView,
    Graph, ProjectIr, ProjectIrFragment, SymbolNode,
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

        merge_workspace_feature_flags(&workspace, &mut ir);
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

    pub fn ir_neighborhood(
        &self,
        seed_node_ids: &[String],
        max_nodes: usize,
        max_hops: usize,
    ) -> Vec<String> {
        if max_nodes == 0 {
            return vec![];
        }

        let known = self.known_node_ids();
        let adjacency = self.adjacency_map();
        let mut queue = VecDeque::<(String, usize)>::new();
        let mut visited = BTreeSet::<String>::new();
        let mut ordered = Vec::<String>::new();

        for seed in seed_node_ids {
            if known.contains(seed) && visited.insert(seed.clone()) {
                queue.push_back((seed.clone(), 0));
            }
        }

        while let Some((node_id, hops)) = queue.pop_front() {
            ordered.push(node_id.clone());
            if ordered.len() >= max_nodes {
                break;
            }

            if hops >= max_hops {
                continue;
            }

            if let Some(neighbors) = adjacency.get(&node_id) {
                for neighbor in neighbors {
                    if visited.insert(neighbor.clone()) {
                        queue.push_back((neighbor.clone(), hops + 1));
                    }
                }
            }
        }

        ordered
    }

    pub fn subgraph_for_nodes(&self, node_ids: &[String]) -> ProjectIrFragment {
        let selected = node_ids
            .iter()
            .filter(|id| !id.trim().is_empty())
            .cloned()
            .collect::<BTreeSet<_>>();
        let includes = |id: &str| selected.contains(id);

        let mut fragment = ProjectIrFragment::default();
        fragment.file_graph.nodes = self
            .file_graph
            .nodes
            .iter()
            .filter(|node| includes(&node.id))
            .cloned()
            .collect();
        fragment.file_graph.nodes.sort_by(|a, b| a.id.cmp(&b.id));
        fragment.file_graph.edges = self
            .file_graph
            .edges
            .iter()
            .filter(|edge| includes(&edge.from) && includes(&edge.to))
            .cloned()
            .collect();
        fragment
            .file_graph
            .edges
            .sort_by(|a, b| (&a.from, &a.to, &a.relation).cmp(&(&b.from, &b.to, &b.relation)));

        fragment.symbol_graph.nodes = self
            .symbol_graph
            .nodes
            .iter()
            .filter(|node| includes(&node.id))
            .cloned()
            .collect();
        fragment.symbol_graph.nodes.sort_by(|a, b| a.id.cmp(&b.id));
        fragment.symbol_graph.edges = self
            .symbol_graph
            .edges
            .iter()
            .filter(|edge| includes(&edge.from) && includes(&edge.to))
            .cloned()
            .collect();
        fragment
            .symbol_graph
            .edges
            .sort_by(|a, b| (&a.from, &a.to, &a.relation).cmp(&(&b.from, &b.to, &b.relation)));

        fragment.feature_graph.nodes = self
            .feature_graph
            .nodes
            .iter()
            .filter(|node| includes(&node.id))
            .cloned()
            .collect();
        fragment.feature_graph.nodes.sort_by(|a, b| a.id.cmp(&b.id));
        fragment.feature_graph.edges = self
            .feature_graph
            .edges
            .iter()
            .filter(|edge| includes(&edge.from) && includes(&edge.to))
            .cloned()
            .collect();
        fragment
            .feature_graph
            .edges
            .sort_by(|a, b| (&a.from, &a.to, &a.relation).cmp(&(&b.from, &b.to, &b.relation)));

        fragment.dataflow_graph.nodes = self
            .dataflow_graph
            .nodes
            .iter()
            .filter(|node| includes(&node.id))
            .cloned()
            .collect();
        fragment
            .dataflow_graph
            .nodes
            .sort_by(|a, b| a.id.cmp(&b.id));
        fragment.dataflow_graph.edges = self
            .dataflow_graph
            .edges
            .iter()
            .filter(|edge| includes(&edge.from) && includes(&edge.to))
            .cloned()
            .collect();
        fragment.dataflow_graph.edges.sort_by(|a, b| {
            (&a.from, &a.to, &a.relation, &a.value_preview).cmp(&(
                &b.from,
                &b.to,
                &b.relation,
                &b.value_preview,
            ))
        });

        fragment.framework_views = self
            .framework_views
            .iter()
            .filter_map(|view| {
                let mut view_nodes = view
                    .node_ids
                    .iter()
                    .filter(|id| includes(id))
                    .cloned()
                    .collect::<Vec<_>>();
                view_nodes.sort();
                view_nodes.dedup();
                (!view_nodes.is_empty()).then(|| FrameworkView {
                    framework: view.framework.clone(),
                    node_ids: view_nodes,
                })
            })
            .collect();
        fragment
            .framework_views
            .sort_by(|a, b| (&a.framework, &a.node_ids).cmp(&(&b.framework, &b.node_ids)));

        fragment
    }

    pub fn context_snippets_for_nodes(
        &self,
        node_ids: &[String],
        max_chars: usize,
    ) -> Vec<ContextSnippet> {
        if max_chars == 0 {
            return vec![];
        }

        let mut remaining = max_chars;
        let mut resolved = BTreeMap::<String, PathBuf>::new();
        for node_id in node_ids {
            if node_id.trim().is_empty() || resolved.contains_key(node_id) {
                continue;
            }

            if let Some(path) = self.resolve_file_for_node_id(node_id) {
                resolved.insert(node_id.clone(), path);
            }
        }

        let mut snippets = Vec::<ContextSnippet>::new();
        for (node_id, file_path) in resolved {
            if remaining == 0 {
                break;
            }

            let max_bytes = (remaining.saturating_mul(4)).clamp(512, 16 * 1024);
            let Some(snippet) = read_file_prefix(&file_path, max_bytes, remaining) else {
                continue;
            };
            if snippet.trim().is_empty() {
                continue;
            }

            remaining = remaining.saturating_sub(snippet.chars().count());
            snippets.push(ContextSnippet {
                node_id,
                file_path,
                snippet,
            });
        }

        snippets
    }

    fn known_node_ids(&self) -> BTreeSet<String> {
        let mut ids = BTreeSet::<String>::new();
        ids.extend(self.file_graph.nodes.iter().map(|node| node.id.clone()));
        ids.extend(self.symbol_graph.nodes.iter().map(|node| node.id.clone()));
        ids.extend(self.feature_graph.nodes.iter().map(|node| node.id.clone()));
        ids.extend(self.dataflow_graph.nodes.iter().map(|node| node.id.clone()));
        ids
    }

    fn adjacency_map(&self) -> BTreeMap<String, BTreeSet<String>> {
        let mut adjacency = BTreeMap::<String, BTreeSet<String>>::new();
        let add_edge =
            |adjacency: &mut BTreeMap<String, BTreeSet<String>>, from: &str, to: &str| {
                adjacency
                    .entry(from.to_string())
                    .or_default()
                    .insert(to.to_string());
                adjacency
                    .entry(to.to_string())
                    .or_default()
                    .insert(from.to_string());
            };

        for edge in &self.file_graph.edges {
            add_edge(&mut adjacency, &edge.from, &edge.to);
        }
        for edge in &self.symbol_graph.edges {
            add_edge(&mut adjacency, &edge.from, &edge.to);
        }
        for edge in &self.feature_graph.edges {
            add_edge(&mut adjacency, &edge.from, &edge.to);
        }
        for edge in &self.dataflow_graph.edges {
            add_edge(&mut adjacency, &edge.from, &edge.to);
        }

        adjacency
    }

    fn resolve_file_for_node_id(&self, node_id: &str) -> Option<PathBuf> {
        if let Some(node) = self.file_graph.nodes.iter().find(|node| node.id == node_id) {
            return Some(node.path.clone());
        }
        if let Some(node) = self
            .symbol_graph
            .nodes
            .iter()
            .find(|node| node.id == node_id)
        {
            return Some(node.file.clone());
        }
        if let Some(node) = self
            .dataflow_graph
            .nodes
            .iter()
            .find(|node| node.id == node_id)
            && let Some(path) = &node.file
        {
            return Some(path.clone());
        }
        if let Some(node) = self
            .feature_graph
            .nodes
            .iter()
            .find(|node| node.id == node_id)
        {
            return parse_feature_source_path(&node.source);
        }
        if let Some(path) = node_id.strip_prefix("file:") {
            let parsed = PathBuf::from(path);
            if parsed.exists() {
                return Some(parsed);
            }
        }

        None
    }
}

fn parse_feature_source_path(source: &str) -> Option<PathBuf> {
    let direct = PathBuf::from(source);
    if direct.exists() {
        return Some(direct);
    }

    let (path, suffix) = source.rsplit_once(':')?;
    if suffix.chars().all(|ch| ch.is_ascii_digit()) {
        let line_scoped = PathBuf::from(path);
        if line_scoped.exists() {
            return Some(line_scoped);
        }
    }

    None
}

fn read_file_prefix(path: &Path, max_bytes: usize, max_chars: usize) -> Option<String> {
    let handle = File::open(path).ok()?;
    let mut bytes = Vec::<u8>::new();
    handle.take(max_bytes as u64).read_to_end(&mut bytes).ok()?;
    Some(
        String::from_utf8_lossy(&bytes)
            .chars()
            .take(max_chars)
            .collect(),
    )
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
    let absolute_root = resolve_workspace_root(root)?;
    let manifest_path = absolute_root.join("Cargo.toml");

    if manifest_path.exists() {
        return WorkspaceAnalyzer::analyze(&absolute_root).with_context(|| {
            format!(
                "analyze Cargo workspace metadata from {}",
                manifest_path.display()
            )
        });
    }

    Ok(synthetic_workspace_from_root(absolute_root))
}

fn resolve_workspace_root(root: &Path) -> Result<PathBuf> {
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
                .and_then(|value| value.parent())
                .context("resolve repository root from crate manifest")?
                .to_path_buf();
            repo_root.join(root)
        }
    };

    Ok(absolute_root)
}

fn synthetic_workspace_from_root(absolute_root: PathBuf) -> CargoWorkspace {
    let crate_name = absolute_root
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| "workspace".to_string());

    CargoWorkspace {
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
    }
}

fn merge_workspace_feature_flags(workspace: &CargoWorkspace, ir: &mut ProjectIr) {
    let mut seen_feature_names = ir
        .feature_graph
        .nodes
        .iter()
        .map(|node| node.name.clone())
        .collect::<HashSet<_>>();

    for member in &workspace.members {
        let source = member.path.join("Cargo.toml").display().to_string();
        let Some(flags) = workspace.feature_flags.get(&member.name) else {
            continue;
        };
        for flag in flags {
            if !seen_feature_names.insert(flag.name.clone()) {
                continue;
            }

            ir.feature_graph.nodes.push(FeatureNode {
                id: format!("feature:{}::{}", member.name, flag.name),
                name: flag.name.clone(),
                source: source.clone(),
            });
        }
    }
}
