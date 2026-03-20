use std::collections::{HashMap, HashSet};

use anyhow::Result;
use audit_agent_core::workspace::CargoWorkspace;
use walkdir::WalkDir;

use crate::LanguageMapper;
use crate::graph::{
    BasicEdge, DataflowEdge, DataflowNode, FeatureNode, FileNode, FrameworkView, FunctionSignature,
    ParameterInfo, ProjectIrFragment, SymbolNode,
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
        let mut feature_nodes_seen = HashSet::<(String, String)>::new();

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
                        qualified_name: function.qualified_name.clone(),
                        file: file.path.clone(),
                        kind: "function".to_string(),
                        line: function.line,
                        signature: function
                            .signature
                            .as_ref()
                            .map(|signature| FunctionSignature {
                                parameters: signature
                                    .parameters
                                    .iter()
                                    .enumerate()
                                    .map(|(position, parameter)| ParameterInfo {
                                        name: parameter.name.clone(),
                                        type_annotation: parameter.type_annotation.clone(),
                                        position,
                                    })
                                    .collect(),
                                return_type: signature.return_type.clone(),
                            }),
                    });
                    fragment.symbol_graph.edges.push(BasicEdge {
                        from: file_id.clone(),
                        to: symbol_id.clone(),
                        relation: "contains".to_string(),
                    });
                    fragment.dataflow_graph.nodes.push(DataflowNode {
                        id: format!("dataflow:{symbol_id}"),
                        label: function.name.clone(),
                        file: Some(file.path.clone()),
                    });
                    symbol_ids_by_name
                        .entry(function.name.clone())
                        .or_default()
                        .push(symbol_id.clone());
                }

                for variable in &file.variables {
                    let variable_dataflow_id = format!(
                        "dataflow:var:{}:{}:{}",
                        file.path.display(),
                        variable.line,
                        variable.name
                    );
                    fragment.dataflow_graph.nodes.push(DataflowNode {
                        id: variable_dataflow_id.clone(),
                        label: variable.name.clone(),
                        file: Some(file.path.clone()),
                    });

                    if let Some(function_name) = &variable.function {
                        let owner_symbol_id =
                            format!("symbol:{}::{}", file.path.display(), function_name);
                        let owner_dataflow_id = format!("dataflow:{owner_symbol_id}");
                        let edge_key = (
                            owner_dataflow_id.clone(),
                            variable_dataflow_id.clone(),
                            "parameter_flow".to_string(),
                        );
                        if dataflow_edges.insert(edge_key) {
                            fragment.dataflow_graph.edges.push(DataflowEdge {
                                from: owner_dataflow_id,
                                to: variable_dataflow_id,
                                relation: "parameter_flow".to_string(),
                                value_preview: Some(format!("let {}", variable.name)),
                            });
                        }
                    }
                }

                for macro_site in &file.macro_sites {
                    let macro_label = format!("{}!", macro_site.macro_name);
                    let macro_symbol_id = format!(
                        "symbol:{}::macro:{}:{}:{}",
                        file.path.display(),
                        macro_site.line,
                        macro_site.column,
                        macro_site.macro_name
                    );
                    fragment.symbol_graph.nodes.push(SymbolNode {
                        id: macro_symbol_id.clone(),
                        name: macro_label,
                        qualified_name: None,
                        file: file.path.clone(),
                        kind: "macro_call".to_string(),
                        line: macro_site.line,
                        signature: None,
                    });
                    fragment.symbol_graph.edges.push(BasicEdge {
                        from: file_id.clone(),
                        to: macro_symbol_id.clone(),
                        relation: "contains".to_string(),
                    });

                    if let Some(caller) = &macro_site.caller {
                        let caller_symbol_id =
                            format!("symbol:{}::{}", file.path.display(), caller);
                        let edge_key = (
                            caller_symbol_id.clone(),
                            macro_symbol_id.clone(),
                            "invokes_macro".to_string(),
                        );
                        if symbol_edges.insert(edge_key) {
                            fragment.symbol_graph.edges.push(BasicEdge {
                                from: caller_symbol_id,
                                to: macro_symbol_id,
                                relation: "invokes_macro".to_string(),
                            });
                        }
                    }
                }

                for trait_impl in &file.trait_impls {
                    let symbol_id = format!(
                        "symbol:{}::impl:{}:{}:{}",
                        file.path.display(),
                        trait_impl.impl_type,
                        trait_impl.method_name,
                        trait_impl.line
                    );
                    fragment.symbol_graph.nodes.push(SymbolNode {
                        id: symbol_id.clone(),
                        name: format!(
                            "{}::{}@{}",
                            trait_impl.trait_name, trait_impl.method_name, trait_impl.impl_type
                        ),
                        qualified_name: Some(format!(
                            "{}::{}",
                            trait_impl.impl_type, trait_impl.method_name
                        )),
                        file: file.path.clone(),
                        kind: "trait_impl_method".to_string(),
                        line: trait_impl.line,
                        signature: None,
                    });
                    fragment.symbol_graph.edges.push(BasicEdge {
                        from: file_id.clone(),
                        to: symbol_id.clone(),
                        relation: "contains".to_string(),
                    });
                    symbol_ids_by_name
                        .entry(trait_impl.method_name.clone())
                        .or_default()
                        .push(symbol_id);
                }

                for divergence in &file.cfg_divergences {
                    let feature_key = (divergence.feature.clone(), file.path.display().to_string());
                    if !feature_nodes_seen.insert(feature_key) {
                        continue;
                    }
                    let feature_id = format!(
                        "feature:{}:{}:{}",
                        divergence.feature,
                        file.path.display(),
                        divergence.line
                    );
                    fragment.feature_graph.nodes.push(FeatureNode {
                        id: feature_id.clone(),
                        name: divergence.feature.clone(),
                        source: format!("{}:{}", file.path.display(), divergence.line),
                    });
                    fragment.feature_graph.edges.push(BasicEdge {
                        from: file_id.clone(),
                        to: feature_id,
                        relation: "cfg_divergence".to_string(),
                    });
                }

                for feature in &file.cfg_features {
                    let feature_key = (feature.clone(), file.path.display().to_string());
                    if !feature_nodes_seen.insert(feature_key) {
                        continue;
                    }
                    let feature_id = format!("feature:{feature}");
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

                            let dataflow_from = format!("dataflow:{from_symbol}");
                            let dataflow_to = format!("dataflow:{to_symbol}");
                            let parameter_flow_key = (
                                dataflow_from.clone(),
                                dataflow_to.clone(),
                                "parameter_flow".to_string(),
                            );
                            if dataflow_edges.insert(parameter_flow_key) {
                                fragment.dataflow_graph.edges.push(DataflowEdge {
                                    from: dataflow_from.clone(),
                                    to: dataflow_to.clone(),
                                    relation: "parameter_flow".to_string(),
                                    value_preview: Some("preview:runtime-value".to_string()),
                                });
                            }

                            let return_flow_key = (
                                dataflow_to.clone(),
                                dataflow_from.clone(),
                                "return_flow".to_string(),
                            );
                            if dataflow_edges.insert(return_flow_key) {
                                fragment.dataflow_graph.edges.push(DataflowEdge {
                                    from: dataflow_to,
                                    to: dataflow_from,
                                    relation: "return_flow".to_string(),
                                    value_preview: Some("preview:return-value".to_string()),
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
