use std::collections::HashMap;
use std::fs;

use anyhow::Result;
use audit_agent_core::workspace::CargoWorkspace;
use walkdir::WalkDir;

use crate::LanguageMapper;
use crate::graph::{BasicEdge, FileNode, FrameworkView, ProjectIrFragment, SymbolNode};

#[derive(Debug, Default)]
pub struct CircomMapper;

impl LanguageMapper for CircomMapper {
    fn can_handle(&self, workspace: &CargoWorkspace) -> bool {
        workspace.members.iter().any(|member| {
            WalkDir::new(&member.path)
                .follow_links(true)
                .into_iter()
                .filter_map(|entry| entry.ok())
                .any(|entry| entry.path().extension().and_then(|v| v.to_str()) == Some("circom"))
        })
    }

    fn build(&self, workspace: &CargoWorkspace) -> Result<ProjectIrFragment> {
        let mut fragment = ProjectIrFragment::default();
        let mut framework_node_ids = Vec::<String>::new();
        for member in &workspace.members {
            for entry in WalkDir::new(&member.path)
                .follow_links(true)
                .into_iter()
                .filter_map(|entry| entry.ok())
            {
                if !entry.file_type().is_file() {
                    continue;
                }
                if entry.path().extension().and_then(|v| v.to_str()) != Some("circom") {
                    continue;
                }
                let file_path = entry.path().to_path_buf();
                let file_id = format!("file:{}", file_path.display());
                fragment.file_graph.nodes.push(FileNode {
                    id: file_id.clone(),
                    path: file_path.clone(),
                    language: "circom".to_string(),
                });

                framework_node_ids.push(file_id.clone());

                let Ok(content) = fs::read_to_string(&file_path) else {
                    continue;
                };
                let (templates, signals) = scan_circom_symbols(&content);
                let mut template_symbol_ids = HashMap::<String, String>::new();

                for template in templates {
                    let template_symbol_id = format!(
                        "symbol:{}::circom_template:{}:{}",
                        file_path.display(),
                        template.name,
                        template.line
                    );
                    fragment.symbol_graph.nodes.push(SymbolNode {
                        id: template_symbol_id.clone(),
                        name: template.name.clone(),
                        file: file_path.clone(),
                        kind: "circom_template".to_string(),
                    });
                    fragment.symbol_graph.edges.push(BasicEdge {
                        from: file_id.clone(),
                        to: template_symbol_id.clone(),
                        relation: "contains".to_string(),
                    });
                    template_symbol_ids.insert(template.name, template_symbol_id.clone());
                    framework_node_ids.push(template_symbol_id);
                }

                for signal in signals {
                    let signal_symbol_id = format!(
                        "symbol:{}::circom_signal:{}:{}:{}",
                        file_path.display(),
                        signal.template,
                        signal.name,
                        signal.line
                    );
                    fragment.symbol_graph.nodes.push(SymbolNode {
                        id: signal_symbol_id.clone(),
                        name: format!("{}::{}", signal.template, signal.name),
                        file: file_path.clone(),
                        kind: format!("circom_signal_{}", signal.kind),
                    });
                    if let Some(template_symbol_id) = template_symbol_ids.get(&signal.template) {
                        // New relation for symbol-level Circom indexing.
                        fragment.symbol_graph.edges.push(BasicEdge {
                            from: template_symbol_id.clone(),
                            to: signal_symbol_id.clone(),
                            relation: "declares_signal".to_string(),
                        });
                    } else {
                        fragment.symbol_graph.edges.push(BasicEdge {
                            from: file_id.clone(),
                            to: signal_symbol_id.clone(),
                            relation: "contains".to_string(),
                        });
                    }
                    framework_node_ids.push(signal_symbol_id);
                }
            }
        }

        if !framework_node_ids.is_empty() {
            framework_node_ids.sort();
            framework_node_ids.dedup();
            fragment.framework_views.push(FrameworkView {
                framework: "circom".to_string(),
                node_ids: framework_node_ids,
            });
        }
        Ok(fragment)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CircomTemplateDecl {
    name: String,
    line: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CircomSignalDecl {
    template: String,
    name: String,
    kind: String,
    line: u32,
}

fn scan_circom_symbols(content: &str) -> (Vec<CircomTemplateDecl>, Vec<CircomSignalDecl>) {
    // Lightweight parser: brace counting does not ignore Circom comments that contain '{' or '}'.
    // This is acceptable for current symbol indexing but should be upgraded for comment-aware scans.
    let mut templates = Vec::<CircomTemplateDecl>::new();
    let mut signals = Vec::<CircomSignalDecl>::new();
    let mut current_template: Option<String> = None;
    let mut template_start_depth = 0i32;
    let mut brace_depth = 0i32;

    for (line_idx, line) in content.lines().enumerate() {
        let line_no = line_idx as u32 + 1;

        if current_template.is_none()
            && let Some(name) = parse_template_start(line)
        {
            template_start_depth = brace_depth;
            current_template = Some(name.clone());
            templates.push(CircomTemplateDecl {
                name,
                line: line_no,
            });
        }

        if let Some(template_name) = &current_template {
            for (kind, signal_name) in parse_signal_decls(line) {
                signals.push(CircomSignalDecl {
                    template: template_name.clone(),
                    name: signal_name,
                    kind,
                    line: line_no,
                });
            }
        }

        brace_depth += line_brace_delta(line);
        if current_template.is_some() && brace_depth <= template_start_depth {
            current_template = None;
        }
    }

    (templates, signals)
}

fn parse_template_start(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    if !trimmed.starts_with("template ") {
        return None;
    }

    let name = trimmed.trim_start_matches("template ").trim_start();
    let ident = name
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
        .collect::<String>();
    (!ident.is_empty()).then_some(ident)
}

fn parse_signal_decls(line: &str) -> Vec<(String, String)> {
    let trimmed = line.trim_start();
    if !trimmed.starts_with("signal ") {
        return vec![];
    }

    let after_signal = trimmed.trim_start_matches("signal ").trim_start();
    let (kind, remainder) = if let Some(rest) = after_signal.strip_prefix("input") {
        ("input", rest.trim_start())
    } else if let Some(rest) = after_signal.strip_prefix("output") {
        ("output", rest.trim_start())
    } else {
        ("intermediate", after_signal)
    };

    let declarations = remainder
        .split(';')
        .next()
        .unwrap_or("")
        .split(',')
        .filter_map(|value| parse_signal_name(value).map(|name| (kind.to_string(), name)))
        .collect::<Vec<_>>();

    declarations
}

fn parse_signal_name(value: &str) -> Option<String> {
    let token = value.trim();
    if token.is_empty() {
        return None;
    }
    let ident = token
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '[' || *ch == ']')
        .collect::<String>();
    let base = ident.split('[').next().unwrap_or("").trim();
    (!base.is_empty()).then(|| base.to_string())
}

fn line_brace_delta(line: &str) -> i32 {
    let mut delta = 0i32;
    for ch in line.chars() {
        if ch == '{' {
            delta += 1;
        } else if ch == '}' {
            delta -= 1;
        }
    }
    delta
}
