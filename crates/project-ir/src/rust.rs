use std::collections::{HashMap, HashSet};

use anyhow::Result;
use audit_agent_core::workspace::CargoWorkspace;
use walkdir::WalkDir;

use crate::LanguageMapper;
use crate::graph::{
    BasicEdge, DataflowEdge, DataflowNode, FeatureNode, FileNode, FrameworkView, ProjectIrFragment,
    SymbolNode,
};
use crate::semantic::build_rust_semantic_index;

#[derive(Debug, Default)]
pub struct RustMapper;

impl LanguageMapper for RustMapper {
    fn can_handle(&self, workspace: &CargoWorkspace) -> bool {
        workspace.members.iter().any(|member| {
            WalkDir::new(&member.path)
                .follow_links(true)
                .into_iter()
                .filter_map(|entry| entry.ok())
                .any(|entry| entry.path().extension().and_then(|v| v.to_str()) == Some("rs"))
        })
    }

    fn build(&self, workspace: &CargoWorkspace) -> Result<ProjectIrFragment> {
        let mut fragment = ProjectIrFragment::default();
        let mut symbol_ids_by_name = HashMap::<String, Vec<String>>::new();
        let mut symbol_edges = HashSet::<(String, String, String)>::new();
        let mut dataflow_edges = HashSet::<(String, String, String)>::new();

        for member in &workspace.members {
            let semantic = build_rust_semantic_index(&member.path)?;
            for file in semantic.files {
                let file_id = format!("file:{}", file.path.display());
                fragment.file_graph.nodes.push(FileNode {
                    id: file_id.clone(),
                    path: file.path.clone(),
                    language: "rust".to_string(),
                });

                for function in &file.functions {
                    let symbol_id = format!("symbol:{}::{}", file.path.display(), function.name);
                    fragment.symbol_graph.nodes.push(SymbolNode {
                        id: symbol_id.clone(),
                        name: function.name.clone(),
                        file: file.path.clone(),
                        kind: "function".to_string(),
                    });
                    fragment.symbol_graph.edges.push(BasicEdge {
                        from: file_id.clone(),
                        to: symbol_id.clone(),
                        relation: "contains".to_string(),
                    });
                    fragment.dataflow_graph.nodes.push(DataflowNode {
                        id: format!("dataflow:{}", symbol_id),
                        label: function.name.clone(),
                        file: Some(file.path.clone()),
                    });
                    symbol_ids_by_name
                        .entry(function.name.clone())
                        .or_default()
                        .push(symbol_id.clone());
                }

                for feature in file.cfg_features {
                    let feature_id = format!("feature:{}", feature);
                    fragment.feature_graph.nodes.push(FeatureNode {
                        id: feature_id.clone(),
                        name: feature.clone(),
                        source: file.path.display().to_string(),
                    });
                    fragment.feature_graph.edges.push(BasicEdge {
                        from: file_id.clone(),
                        to: feature_id,
                        relation: "cfg".to_string(),
                    });
                }

                for call in &file.function_calls {
                    let from_symbol = format!("symbol:{}::{}", file.path.display(), call.caller);
                    if let Some(to_symbols) = symbol_ids_by_name.get(&call.callee) {
                        for to_symbol in to_symbols {
                            let symbol_key =
                                (from_symbol.clone(), to_symbol.clone(), "calls".to_string());
                            if symbol_edges.insert(symbol_key) {
                                fragment.symbol_graph.edges.push(BasicEdge {
                                    from: from_symbol.clone(),
                                    to: to_symbol.clone(),
                                    relation: "calls".to_string(),
                                });
                            }

                            let dataflow_from = format!("dataflow:{}", from_symbol);
                            let dataflow_to = format!("dataflow:{}", to_symbol);
                            let dataflow_key = (
                                dataflow_from.clone(),
                                dataflow_to.clone(),
                                "call-arg-flow".to_string(),
                            );
                            if dataflow_edges.insert(dataflow_key) {
                                fragment.dataflow_graph.edges.push(DataflowEdge {
                                    from: dataflow_from,
                                    to: dataflow_to,
                                    relation: "call-arg-flow".to_string(),
                                    value_preview: Some("preview:runtime-value".to_string()),
                                });
                            }
                        }
                    }
                }
            }
        }

        let framework_nodes = fragment
            .file_graph
            .nodes
            .iter()
            .map(|node| node.id.clone())
            .collect::<Vec<_>>();
        if !framework_nodes.is_empty() {
            fragment.framework_views.push(FrameworkView {
                framework: "rust".to_string(),
                node_ids: framework_nodes,
            });
        }

        Ok(fragment)
    }
}
