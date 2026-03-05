use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::semantic::ra_client::SemanticIndex;

pub type ChipName = String;
pub type ColumnName = String;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MethodSpan {
    pub file: PathBuf,
    pub line_start: u32,
    pub line_end: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChipNode {
    pub name: ChipName,
    pub crate_name: String,
    pub configure_span: Option<MethodSpan>,
    pub synthesize_span: Option<MethodSpan>,
    pub columns: Vec<ColumnName>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CdgEdge {
    pub from_chip: ChipName,
    pub to_chip: ChipName,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskAnnotation {
    IsolatedNode {
        chip: ChipName,
        column: ColumnName,
    },
    RangeGap {
        from_chip: ChipName,
        to_chip: ChipName,
        gap: String,
    },
    SelectorConflict {
        chip_a: ChipName,
        chip_b: ChipName,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConstraintDependencyGraph {
    pub chips: Vec<ChipNode>,
    pub edges: Vec<CdgEdge>,
    pub risk_annotations: Vec<RiskAnnotation>,
}

#[derive(Debug, Clone)]
struct ChipState {
    node: ChipNode,
    configure_body: Option<String>,
    synthesize_body: Option<String>,
    selectors: Vec<String>,
}

impl ConstraintDependencyGraph {
    pub fn build(semantic_index: &SemanticIndex) -> Result<Self> {
        let mut chips = HashMap::<ChipName, ChipState>::new();

        for fn_ref in semantic_index.find_trait_impls("Chip", "configure") {
            let (span, body) = extract_method_block(&fn_ref.file, fn_ref.line, "configure")
                .with_context(|| {
                    format!(
                        "failed to extract configure block for {} in {}",
                        fn_ref.impl_type,
                        fn_ref.file.display()
                    )
                })?;

            let columns = detect_column_variables(&body);
            let selectors = detect_selector_variables(&body);

            let state = chips
                .entry(fn_ref.impl_type.clone())
                .or_insert_with(|| ChipState {
                    node: ChipNode {
                        name: fn_ref.impl_type.clone(),
                        crate_name: fn_ref.crate_name.clone(),
                        configure_span: None,
                        synthesize_span: None,
                        columns: vec![],
                    },
                    configure_body: None,
                    synthesize_body: None,
                    selectors: vec![],
                });
            state.node.configure_span = Some(span);
            state.node.columns = columns;
            state.selectors = selectors;
            state.configure_body = Some(body);
        }

        for fn_ref in semantic_index.find_trait_impls("Chip", "synthesize") {
            let (span, body) = extract_method_block(&fn_ref.file, fn_ref.line, "synthesize")
                .with_context(|| {
                    format!(
                        "failed to extract synthesize block for {} in {}",
                        fn_ref.impl_type,
                        fn_ref.file.display()
                    )
                })?;

            let state = chips
                .entry(fn_ref.impl_type.clone())
                .or_insert_with(|| ChipState {
                    node: ChipNode {
                        name: fn_ref.impl_type.clone(),
                        crate_name: fn_ref.crate_name.clone(),
                        configure_span: None,
                        synthesize_span: None,
                        columns: vec![],
                    },
                    configure_body: None,
                    synthesize_body: None,
                    selectors: vec![],
                });
            state.node.synthesize_span = Some(span);
            state.synthesize_body = Some(body);
        }

        let chip_names = chips.keys().cloned().collect::<Vec<_>>();
        let mut edges = vec![];
        let mut edge_keys = HashSet::<(String, String)>::new();

        for (consumer_chip, state) in &chips {
            let body = merge_bodies(
                state.configure_body.as_deref(),
                state.synthesize_body.as_deref(),
            );
            for provider_chip in &chip_names {
                if provider_chip == consumer_chip {
                    continue;
                }
                if body.contains(&format!("{provider_chip}::")) {
                    let key = (provider_chip.clone(), consumer_chip.clone());
                    if edge_keys.insert(key.clone()) {
                        edges.push(CdgEdge {
                            from_chip: key.0,
                            to_chip: key.1,
                            reason: "chip dependency via configure/synthesize call".to_string(),
                        });
                    }
                }
            }
        }

        let mut risk_annotations = vec![];
        let mut isolated_columns = HashMap::<String, Vec<String>>::new();

        for (chip_name, state) in &chips {
            let combined = merge_bodies(
                state.configure_body.as_deref(),
                state.synthesize_body.as_deref(),
            );
            for column in &state.node.columns {
                if !has_constraining_usage(&combined, column) {
                    isolated_columns
                        .entry(chip_name.clone())
                        .or_default()
                        .push(column.clone());
                    risk_annotations.push(RiskAnnotation::IsolatedNode {
                        chip: chip_name.clone(),
                        column: column.clone(),
                    });
                }
            }
        }

        for edge in &edges {
            if edge.from_chip.contains("RangeCheck")
                && isolated_columns
                    .get(&edge.to_chip)
                    .is_some_and(|columns| !columns.is_empty())
            {
                risk_annotations.push(RiskAnnotation::RangeGap {
                    from_chip: edge.from_chip.clone(),
                    to_chip: edge.to_chip.clone(),
                    gap: "consumer has columns without obvious range constraints".to_string(),
                });
            }
        }

        let mut selector_to_chips = HashMap::<String, Vec<String>>::new();
        for (chip_name, state) in &chips {
            for selector in &state.selectors {
                selector_to_chips
                    .entry(selector.clone())
                    .or_default()
                    .push(chip_name.clone());
            }
        }

        let mut selector_conflict_pairs = BTreeSet::<(String, String)>::new();
        for chips_for_selector in selector_to_chips.values() {
            if chips_for_selector.len() < 2 {
                continue;
            }
            for i in 0..chips_for_selector.len() {
                for j in (i + 1)..chips_for_selector.len() {
                    let a = chips_for_selector[i].clone();
                    let b = chips_for_selector[j].clone();
                    if a <= b {
                        selector_conflict_pairs.insert((a, b));
                    } else {
                        selector_conflict_pairs.insert((b, a));
                    }
                }
            }
        }

        for (chip_a, chip_b) in selector_conflict_pairs {
            risk_annotations.push(RiskAnnotation::SelectorConflict { chip_a, chip_b });
        }

        let mut chip_nodes = chips.into_values().map(|s| s.node).collect::<Vec<_>>();
        chip_nodes.sort_by(|a, b| a.name.cmp(&b.name));
        edges.sort_by(|a, b| {
            (a.from_chip.as_str(), a.to_chip.as_str())
                .cmp(&(b.from_chip.as_str(), b.to_chip.as_str()))
        });

        Ok(Self {
            chips: chip_nodes,
            edges,
            risk_annotations,
        })
    }

    pub fn high_risk_nodes(&self) -> Vec<&ChipNode> {
        self.chips
            .iter()
            .filter(|chip| {
                self.risk_annotations
                    .iter()
                    .any(|annotation| annotation_mentions_chip(annotation, &chip.name))
            })
            .collect()
    }

    pub fn to_dot(&self) -> String {
        let mut lines = vec!["digraph cdg {".to_string(), "  rankdir=LR;".to_string()];

        let high_risk = self
            .high_risk_nodes()
            .into_iter()
            .map(|chip| chip.name.clone())
            .collect::<HashSet<_>>();

        for chip in &self.chips {
            let attrs = if high_risk.contains(&chip.name) {
                " [color=red, penwidth=2]"
            } else {
                ""
            };
            lines.push(format!("  \"{}\"{};", chip.name, attrs));
        }

        for edge in &self.edges {
            lines.push(format!(
                "  \"{}\" -> \"{}\" [label=\"{}\"];",
                edge.from_chip, edge.to_chip, edge.reason
            ));
        }

        lines.push("}".to_string());
        lines.join("\n")
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string())
    }
}

fn annotation_mentions_chip(annotation: &RiskAnnotation, chip: &str) -> bool {
    match annotation {
        RiskAnnotation::IsolatedNode { chip: c, .. } => c == chip,
        RiskAnnotation::RangeGap {
            from_chip, to_chip, ..
        } => from_chip == chip || to_chip == chip,
        RiskAnnotation::SelectorConflict { chip_a, chip_b } => chip_a == chip || chip_b == chip,
    }
}

fn merge_bodies(configure: Option<&str>, synthesize: Option<&str>) -> String {
    [
        configure.unwrap_or_default(),
        synthesize.unwrap_or_default(),
    ]
    .join("\n")
}

fn extract_method_block(
    file: &PathBuf,
    start_line: u32,
    method_name: &str,
) -> Result<(MethodSpan, String)> {
    let source =
        fs::read_to_string(file).with_context(|| format!("failed to read {}", file.display()))?;
    let lines = source.lines().collect::<Vec<_>>();

    if lines.is_empty() {
        return Ok((
            MethodSpan {
                file: file.clone(),
                line_start: start_line,
                line_end: start_line,
            },
            String::new(),
        ));
    }

    let mut fn_line_idx = usize::try_from(start_line.saturating_sub(1)).unwrap_or(0);
    if fn_line_idx >= lines.len() {
        fn_line_idx = lines.len() - 1;
    }

    let fn_marker = format!("fn {method_name}");
    while fn_line_idx > 0 && !lines[fn_line_idx].contains(&fn_marker) {
        fn_line_idx -= 1;
    }
    while fn_line_idx < lines.len() && !lines[fn_line_idx].contains(&fn_marker) {
        fn_line_idx += 1;
        if fn_line_idx == lines.len() {
            fn_line_idx = usize::try_from(start_line.saturating_sub(1))
                .unwrap_or(0)
                .min(lines.len() - 1);
            break;
        }
    }

    let mut end_idx = fn_line_idx;
    let mut brace_depth = 0i32;
    let mut seen_open = false;

    for (idx, line) in lines.iter().enumerate().skip(fn_line_idx) {
        for ch in line.chars() {
            if ch == '{' {
                seen_open = true;
                brace_depth += 1;
            } else if ch == '}' {
                brace_depth -= 1;
            }
        }

        if seen_open && brace_depth <= 0 {
            end_idx = idx;
            break;
        }
    }

    let body = lines[fn_line_idx..=end_idx].join("\n");
    Ok((
        MethodSpan {
            file: file.clone(),
            line_start: fn_line_idx as u32 + 1,
            line_end: end_idx as u32 + 1,
        },
        body,
    ))
}

fn detect_column_variables(body: &str) -> Vec<String> {
    body.lines()
        .filter_map(|line| {
            let line = line.trim();
            if !(line.contains("advice_column(")
                || line.contains("instance_column(")
                || line.contains("fixed_column("))
            {
                return None;
            }
            parse_let_var_name(line)
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn detect_selector_variables(body: &str) -> Vec<String> {
    body.lines()
        .filter_map(|line| {
            let line = line.trim();
            if !line.contains("selector(") {
                return None;
            }
            parse_let_var_name(line)
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn parse_let_var_name(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    if !trimmed.starts_with("let ") {
        return None;
    }
    let after_let = trimmed.trim_start_matches("let ");
    let raw_name = after_let
        .split('=')
        .next()? // left side of assignment
        .trim()
        .trim_start_matches("mut ")
        .trim();
    let cleaned = raw_name
        .trim_start_matches('&')
        .trim_start_matches("mut ")
        .trim();

    if cleaned.is_empty() || cleaned == "_" {
        None
    } else {
        Some(cleaned.to_string())
    }
}

fn has_constraining_usage(body: &str, variable: &str) -> bool {
    let anchors = [
        "create_gate",
        "lookup",
        "constrain_equal",
        "range_check",
        "assign_advice",
        "copy_advice",
        "enable",
    ];

    let declaration_prefix = format!("let {variable}");
    for line in body.lines() {
        if !contains_word(line, variable) {
            continue;
        }
        if line.contains(&declaration_prefix) {
            continue;
        }
        if anchors.iter().any(|anchor| line.contains(anchor)) {
            return true;
        }
    }

    false
}

fn contains_word(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }

    haystack
        .match_indices(needle)
        .any(|(idx, _)| is_word_boundary(haystack, idx, idx + needle.len()))
}

fn is_word_boundary(text: &str, start: usize, end: usize) -> bool {
    let bytes = text.as_bytes();
    let left_ok = if start == 0 {
        true
    } else {
        !is_ident_char(bytes[start - 1])
    };
    let right_ok = if end >= bytes.len() {
        true
    } else {
        !is_ident_char(bytes[end])
    };
    left_ok && right_ok
}

fn is_ident_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}
