use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;

use project_ir::ProjectIr;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplorerGraphResponse {
    pub session_id: String,
    pub nodes: Vec<ExplorerNodeResponse>,
    pub edges: Vec<ExplorerEdgeResponse>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplorerNodeResponse {
    pub id: String,
    pub label: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<FunctionSignatureResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub child_count: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionSignatureResponse {
    pub parameters: Vec<ParameterInfoResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_type: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParameterInfoResponse {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_annotation: Option<String>,
    pub position: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplorerEdgeResponse {
    pub from: String,
    pub to: String,
    pub relation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameter_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameter_position: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_preview: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExplorerDepth {
    Overview,
    Full,
}

/// FNV-1a hash, stable across Rust versions. Produces a 12-hex-char
/// suffix (48 bits) to keep collision probability negligible up to ~16M nodes.
pub fn hash_id(prefix: &str, qualified_path: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for byte in qualified_path.bytes() {
        h ^= byte as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{}_{:012x}", prefix, h & 0xFFFF_FFFF_FFFF)
}

pub struct ExplorerGraphBuilder<'a> {
    ir: &'a ProjectIr,
    root: &'a Path,
}

impl<'a> ExplorerGraphBuilder<'a> {
    pub fn new(ir: &'a ProjectIr, root: &'a Path) -> Self {
        Self { ir, root }
    }

    /// Build the explorer graph, then filter by depth/cluster.
    pub fn build(
        &self,
        session_id: &str,
        depth: ExplorerDepth,
        cluster: Option<&str>,
    ) -> Result<ExplorerGraphResponse, ExplorerBuildError> {
        let (mut nodes, mut edges) = self.build_hierarchy();

        let is_overview = cluster.is_none() && depth == ExplorerDepth::Overview;
        if !is_overview {
            self.add_symbols(&mut nodes, &mut edges);
            self.add_cross_file_edges(&mut edges);
        }

        Self::dedupe_nodes_and_edges(&mut nodes, &mut edges);
        self.assign_hash_ids(&mut nodes, &mut edges);
        self.attach_child_counts(&mut nodes, &edges);

        if let Some(cluster_id) = cluster {
            if !nodes.iter().any(|n| n.id == cluster_id) {
                return Err(ExplorerBuildError::UnknownCluster(cluster_id.to_string()));
            }
            self.filter_to_cluster(&mut nodes, &mut edges, cluster_id);
        }

        Ok(ExplorerGraphResponse {
            session_id: session_id.to_string(),
            nodes,
            edges,
        })
    }

    /// Detect crates and infer module/file hierarchy with contains edges.
    fn build_hierarchy(&self) -> (Vec<ExplorerNodeResponse>, Vec<ExplorerEdgeResponse>) {
        let mut nodes = Vec::new();
        let mut edges = Vec::new();

        let mut seen_crates: HashMap<String, String> = HashMap::new();
        let mut seen_modules: HashMap<String, String> = HashMap::new();

        for file_node in &self.ir.file_graph.nodes {
            let rel_path = file_node
                .path
                .strip_prefix(self.root)
                .unwrap_or(&file_node.path);
            let rel_str = rel_path.to_string_lossy().replace('\\', "/");
            if rel_str.is_empty() {
                continue;
            }
            let segments: Vec<&str> = rel_str.split('/').collect();
            if segments.is_empty() {
                continue;
            }

            let crate_name = segments[0].to_string();
            let crate_temp_id = format!("__crate:{crate_name}");
            if !seen_crates.contains_key(&crate_name) {
                seen_crates.insert(crate_name.clone(), crate_temp_id.clone());
                nodes.push(ExplorerNodeResponse {
                    id: crate_temp_id.clone(),
                    label: crate_name,
                    kind: "crate".to_string(),
                    file_path: None,
                    line: None,
                    signature: None,
                    child_count: None,
                });
            }

            let file_temp_id = format!("__file:{rel_str}");
            let file_label = segments.last().copied().unwrap_or_default().to_string();
            nodes.push(ExplorerNodeResponse {
                id: file_temp_id.clone(),
                label: file_label,
                kind: "file".to_string(),
                file_path: Some(rel_str.clone()),
                line: None,
                signature: None,
                child_count: None,
            });

            if segments.len() > 2 {
                let dir_path = segments[..segments.len() - 1].join("/");
                let module_temp_id = format!("__module:{dir_path}");

                if !seen_modules.contains_key(&dir_path) {
                    seen_modules.insert(dir_path.clone(), module_temp_id.clone());
                    let module_label = segments[segments.len() - 2].to_string();
                    nodes.push(ExplorerNodeResponse {
                        id: module_temp_id.clone(),
                        label: module_label,
                        kind: "module".to_string(),
                        file_path: None,
                        line: None,
                        signature: None,
                        child_count: None,
                    });

                    edges.push(ExplorerEdgeResponse {
                        from: crate_temp_id.clone(),
                        to: module_temp_id.clone(),
                        relation: "contains".to_string(),
                        parameter_name: None,
                        parameter_position: None,
                        value_preview: None,
                    });
                }

                let parent_module_id = seen_modules
                    .get(&dir_path)
                    .cloned()
                    .unwrap_or(crate_temp_id.clone());
                edges.push(ExplorerEdgeResponse {
                    from: parent_module_id,
                    to: file_temp_id,
                    relation: "contains".to_string(),
                    parameter_name: None,
                    parameter_position: None,
                    value_preview: None,
                });
            } else {
                edges.push(ExplorerEdgeResponse {
                    from: crate_temp_id,
                    to: file_temp_id,
                    relation: "contains".to_string(),
                    parameter_name: None,
                    parameter_position: None,
                    value_preview: None,
                });
            }
        }

        (nodes, edges)
    }

    /// Add symbol nodes from symbol_graph with function signatures and file->symbol contains edges.
    fn add_symbols(
        &self,
        nodes: &mut Vec<ExplorerNodeResponse>,
        edges: &mut Vec<ExplorerEdgeResponse>,
    ) {
        for sym in &self.ir.symbol_graph.nodes {
            let rel_path = sym.file.strip_prefix(self.root).unwrap_or(&sym.file);
            let rel_str = rel_path.to_string_lossy().replace('\\', "/");
            let sym_temp_id = format!("__symbol:{rel_str}::{}::{}", sym.name, sym.line);
            let file_temp_id = format!("__file:{rel_str}");

            let signature = sym.signature.as_ref().map(|sig| FunctionSignatureResponse {
                parameters: sig
                    .parameters
                    .iter()
                    .map(|p| ParameterInfoResponse {
                        name: p.name.clone(),
                        type_annotation: p.type_annotation.clone(),
                        position: p.position as u32,
                    })
                    .collect(),
                return_type: sig.return_type.clone(),
            });

            nodes.push(ExplorerNodeResponse {
                id: sym_temp_id.clone(),
                label: sym.name.clone(),
                kind: sym.kind.clone(),
                file_path: Some(rel_str),
                line: Some(sym.line),
                signature,
                child_count: None,
            });

            edges.push(ExplorerEdgeResponse {
                from: file_temp_id,
                to: sym_temp_id,
                relation: "contains".to_string(),
                parameter_name: None,
                parameter_position: None,
                value_preview: None,
            });
        }
    }

    /// Add cross-file edges from symbol and dataflow graphs.
    fn add_cross_file_edges(&self, edges: &mut Vec<ExplorerEdgeResponse>) {
        let sym_id_map: HashMap<String, String> = self
            .ir
            .symbol_graph
            .nodes
            .iter()
            .map(|sym| {
                let rel_path = sym.file.strip_prefix(self.root).unwrap_or(&sym.file);
                let rel_str = rel_path.to_string_lossy().replace('\\', "/");
                (
                    sym.id.clone(),
                    format!("__symbol:{rel_str}::{}::{}", sym.name, sym.line),
                )
            })
            .collect();

        for edge in &self.ir.symbol_graph.edges {
            if let (Some(from_id), Some(to_id)) =
                (sym_id_map.get(&edge.from), sym_id_map.get(&edge.to))
            {
                edges.push(ExplorerEdgeResponse {
                    from: from_id.clone(),
                    to: to_id.clone(),
                    relation: edge.relation.clone(),
                    parameter_name: None,
                    parameter_position: None,
                    value_preview: None,
                });
            }
        }

        for edge in &self.ir.dataflow_graph.edges {
            if let (Some(from_id), Some(to_id)) =
                (sym_id_map.get(&edge.from), sym_id_map.get(&edge.to))
            {
                edges.push(ExplorerEdgeResponse {
                    from: from_id.clone(),
                    to: to_id.clone(),
                    relation: edge.relation.clone(),
                    parameter_name: None,
                    parameter_position: None,
                    value_preview: edge.value_preview.clone(),
                });
            }
        }
    }

    fn dedupe_nodes_and_edges(
        nodes: &mut Vec<ExplorerNodeResponse>,
        edges: &mut Vec<ExplorerEdgeResponse>,
    ) {
        let mut seen_node_ids = HashSet::new();
        nodes.retain(|node| seen_node_ids.insert(node.id.clone()));

        let mut seen_edge_keys = HashSet::new();
        edges.retain(|edge| {
            let key = (
                edge.from.clone(),
                edge.to.clone(),
                edge.relation.clone(),
                edge.parameter_name.clone(),
                edge.parameter_position,
                edge.value_preview.clone(),
            );
            seen_edge_keys.insert(key)
        });
    }

    /// Replace temporary IDs with stable hash IDs.
    fn assign_hash_ids(
        &self,
        nodes: &mut [ExplorerNodeResponse],
        edges: &mut [ExplorerEdgeResponse],
    ) {
        let mut id_map: HashMap<String, String> = HashMap::new();

        for node in nodes.iter_mut() {
            let prefix = match node.kind.as_str() {
                "crate" => "crt",
                "module" => "mod",
                "file" => "fil",
                _ => "sym",
            };
            let hashed = hash_id(prefix, &node.id);
            id_map.insert(node.id.clone(), hashed.clone());
            node.id = hashed;
        }

        for edge in edges.iter_mut() {
            if let Some(new_from) = id_map.get(&edge.from) {
                edge.from = new_from.clone();
            }
            if let Some(new_to) = id_map.get(&edge.to) {
                edge.to = new_to.clone();
            }
        }
    }

    /// Count direct children per cluster node.
    fn attach_child_counts(
        &self,
        nodes: &mut [ExplorerNodeResponse],
        edges: &[ExplorerEdgeResponse],
    ) {
        let mut counts: HashMap<String, u32> = HashMap::new();
        for edge in edges {
            if edge.relation == "contains" {
                *counts.entry(edge.from.clone()).or_insert(0) += 1;
            }
        }

        for node in nodes.iter_mut() {
            if matches!(node.kind.as_str(), "crate" | "module" | "file") {
                node.child_count = Some(*counts.get(&node.id).unwrap_or(&0));
            }
        }
    }

    /// Keep only descendants of a specific cluster and their interconnecting edges.
    fn filter_to_cluster(
        &self,
        nodes: &mut Vec<ExplorerNodeResponse>,
        edges: &mut Vec<ExplorerEdgeResponse>,
        cluster_id: &str,
    ) {
        let mut children_of: HashMap<&str, Vec<&str>> = HashMap::new();
        for edge in edges.iter() {
            if edge.relation == "contains" {
                children_of
                    .entry(edge.from.as_str())
                    .or_default()
                    .push(edge.to.as_str());
            }
        }

        let mut descendants = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(cluster_id);

        while let Some(current) = queue.pop_front() {
            if let Some(children) = children_of.get(current) {
                for child in children {
                    if descendants.insert((*child).to_string()) {
                        queue.push_back(child);
                    }
                }
            }
        }

        nodes.retain(|node| descendants.contains(&node.id));
        edges.retain(|edge| descendants.contains(&edge.from) && descendants.contains(&edge.to));
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ExplorerBuildError {
    #[error("Unknown cluster node ID: {0}")]
    UnknownCluster(String),
}

#[cfg(test)]
#[path = "explorer_graph_tests.rs"]
mod tests;
