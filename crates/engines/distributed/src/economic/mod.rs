use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use audit_agent_core::finding::{
    CodeLocation, Evidence, Finding, FindingCategory, FindingId, FindingStatus, Framework,
    Severity, VerificationStatus,
};
use audit_agent_core::workspace::CargoWorkspace;
use engine_crypto::semantic::ra_client::SemanticIndex;
use llm::{LlmProvider, LlmRole, role_aware_llm_call};
use serde::Deserialize;
use tree_sitter::{Node, Parser};
use walkdir::WalkDir;

pub struct EconomicAttackChecker {
    checklist: Vec<EconomicAttackVector>,
    llm: Option<Arc<dyn LlmProvider>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize)]
pub enum EconCategory {
    Sequencer,
    Prover,
    Sybil,
    Bridge,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct EconomicAttackVector {
    pub id: String,
    pub name: String,
    pub category: EconCategory,
    pub detection: EconDetection,
    #[serde(default = "default_observation_severity")]
    pub severity: Severity,
    #[serde(default)]
    pub spec_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "type")]
pub enum EconDetection {
    CallSiteAbsent {
        fn_patterns: Vec<String>,
        description: String,
    },
    CallSitePresent {
        fn_patterns: Vec<String>,
        description: String,
    },
    StructFieldAbsent {
        struct_name: String,
        field: String,
        description: String,
    },
    ConfigBoundCheck {
        const_name: String,
        required_bound: ConfigBound,
        description: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
pub struct ConfigBound {
    #[serde(default)]
    pub exists: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DetectionOutcome {
    triggered: bool,
    description: String,
    location: CodeLocation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SymbolHit {
    symbol: String,
    location: CodeLocation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StructFieldHit {
    struct_name: String,
    field: String,
    location: CodeLocation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ConstHit {
    name: String,
    value: Option<String>,
    location: CodeLocation,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct WorkspaceScan {
    calls: Vec<SymbolHit>,
    struct_fields: Vec<StructFieldHit>,
    consts: Vec<ConstHit>,
}

#[derive(Debug, Clone, Deserialize)]
struct ChecklistFile {
    vectors: Vec<EconomicAttackVector>,
}

fn default_observation_severity() -> Severity {
    Severity::Observation
}

impl EconomicAttackChecker {
    pub fn load_from_dir(rules_dir: &Path, llm: Option<Arc<dyn LlmProvider>>) -> Result<Self> {
        let mut checklist = Vec::<EconomicAttackVector>::new();
        for entry in WalkDir::new(rules_dir)
            .into_iter()
            .filter_map(|entry| entry.ok())
        {
            if !entry.file_type().is_file() {
                continue;
            }
            let ext = entry
                .path()
                .extension()
                .and_then(|ext| ext.to_str())
                .unwrap_or_default();
            if ext != "yaml" && ext != "yml" {
                continue;
            }

            let content = fs::read_to_string(entry.path()).with_context(|| {
                format!(
                    "failed to read economic checklist file {}",
                    entry.path().display()
                )
            })?;
            let parsed: ChecklistFile = serde_yaml::from_str(&content).with_context(|| {
                format!("invalid economic checklist file {}", entry.path().display())
            })?;
            checklist.extend(parsed.vectors);
        }

        checklist.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(Self { checklist, llm })
    }

    pub fn vectors(&self) -> &[EconomicAttackVector] {
        &self.checklist
    }

    pub async fn analyze(
        &self,
        workspace: &CargoWorkspace,
        semantic_index: &SemanticIndex,
    ) -> Vec<Finding> {
        let scan = scan_workspace(workspace).unwrap_or_default();
        let mut findings = Vec::<Finding>::new();

        for vector in &self.checklist {
            let outcome = run_detection(&vector.detection, workspace, semantic_index, &scan);
            if !outcome.triggered {
                continue;
            }

            let description = self.render_description(vector, &outcome).await;
            findings.push(build_finding(vector, &outcome, description));
        }
        findings
    }

    async fn render_description(
        &self,
        vector: &EconomicAttackVector,
        outcome: &DetectionOutcome,
    ) -> String {
        let default_description = outcome.description.clone();
        let Some(llm) = &self.llm else {
            return default_description;
        };

        let safe_name = sanitize_prompt_input(&vector.name);
        let safe_summary = sanitize_prompt_input(&default_description);
        let prompt = format!(
            "Improve readability of this economic risk note without changing technical content.\n\
             Vector: {} - {}\n\
             Evidence summary: {}\n\
             Output only improved text.",
            vector.id, safe_name, safe_summary
        );
        match role_aware_llm_call(llm.as_ref(), LlmRole::ProseRendering, &prompt).await {
            Ok((response, provenance)) => {
                tracing::debug!(
                    provider = %provenance.provider,
                    model = ?provenance.model,
                    role = %provenance.role,
                    duration_ms = provenance.duration_ms,
                    attempt = provenance.attempt,
                    "captured economic-description LLM provenance"
                );
                response
            }
            Err(_) => default_description,
        }
    }
}

fn build_finding(
    vector: &EconomicAttackVector,
    outcome: &DetectionOutcome,
    description: String,
) -> Finding {
    let mut tool_versions = HashMap::new();
    tool_versions.insert("economic_checker".to_string(), "tree-sitter".to_string());

    let references = if vector.spec_refs.is_empty() {
        String::new()
    } else {
        format!(" References: {}.", vector.spec_refs.join(", "))
    };

    Finding {
        id: FindingId::new(vector.id.clone()),
        title: vector.name.clone(),
        severity: Severity::Observation,
        category: FindingCategory::Incentive,
        framework: Framework::Static,
        affected_components: vec![outcome.location.clone()],
        prerequisites: "Protocol-level economic assumptions are deployed in production."
            .to_string(),
        exploit_path: format!(
            "Deterministic checklist trigger fired for vector {}.",
            vector.id
        ),
        impact: description.clone(),
        evidence: Evidence {
            command: None,
            seed: None,
            trace_file: None,
            counterexample: None,
            harness_path: None,
            smt2_file: None,
            container_digest: "n/a".to_string(),
            tool_versions,
        },
        evidence_gate_level: 0,
        llm_generated: false,
        recommendation: format!(
            "Review protocol controls and add explicit guard rails for {}.{}",
            vector.name, references
        ),
        regression_test: None,
        status: FindingStatus::Open,
        regression_check: false,
        verification_status: VerificationStatus::Unverified {
            reason: "Economic attack analysis - no formal proof. Requires manual protocol review."
                .to_string(),
        },
    }
}

fn run_detection(
    detection: &EconDetection,
    workspace: &CargoWorkspace,
    semantic_index: &SemanticIndex,
    scan: &WorkspaceScan,
) -> DetectionOutcome {
    let fallback_location = default_location(workspace);
    match detection {
        EconDetection::CallSiteAbsent {
            fn_patterns,
            description,
        } => {
            let matches = find_matching_calls(fn_patterns, &scan.calls);
            let triggered =
                workspace_has_compiled_code(workspace, semantic_index, scan) && matches.is_empty();
            DetectionOutcome {
                triggered,
                description: description.clone(),
                location: fallback_location,
            }
        }
        EconDetection::CallSitePresent {
            fn_patterns,
            description,
        } => {
            let matches = find_matching_calls(fn_patterns, &scan.calls);
            let location = matches
                .first()
                .map(|hit| hit.location.clone())
                .unwrap_or(fallback_location);
            DetectionOutcome {
                triggered: !matches.is_empty(),
                description: description.clone(),
                location,
            }
        }
        EconDetection::StructFieldAbsent {
            struct_name,
            field,
            description,
        } => {
            let present = scan
                .struct_fields
                .iter()
                .any(|hit| hit.struct_name == *struct_name && hit.field == *field);
            DetectionOutcome {
                triggered: !present,
                description: description.clone(),
                location: fallback_location,
            }
        }
        EconDetection::ConfigBoundCheck {
            const_name,
            required_bound,
            description,
        } => {
            let present = scan
                .consts
                .iter()
                .find(|hit| const_matches(&hit.name, const_name));
            let triggered = if required_bound.exists {
                present.is_none()
            } else {
                present.is_some()
            };
            let location = present
                .map(|hit| hit.location.clone())
                .unwrap_or(fallback_location);
            DetectionOutcome {
                triggered,
                description: description.clone(),
                location,
            }
        }
    }
}

fn workspace_has_compiled_code(
    workspace: &CargoWorkspace,
    semantic_index: &SemanticIndex,
    scan: &WorkspaceScan,
) -> bool {
    let has_rust_targets = workspace.members.iter().any(|member| {
        member.path.join("src/lib.rs").exists() || member.path.join("src/main.rs").exists()
    });

    let has_semantic_calls = semantic_index
        .call_graph
        .values()
        .any(|callees| !callees.is_empty());
    let has_ast_calls = !scan.calls.is_empty();

    has_rust_targets && (has_semantic_calls || has_ast_calls)
}

fn find_matching_calls<'a>(patterns: &[String], calls: &'a [SymbolHit]) -> Vec<&'a SymbolHit> {
    calls
        .iter()
        .filter(|call| {
            patterns
                .iter()
                .any(|pattern| call_pattern_matches(&call.symbol, pattern))
        })
        .collect()
}

fn call_pattern_matches(symbol: &str, pattern: &str) -> bool {
    let symbol = normalize_symbol(symbol);
    let pattern = normalize_symbol(pattern);
    if symbol.contains(&pattern) {
        return true;
    }

    let symbol_tail = symbol.rsplit("::").next().unwrap_or(symbol.as_str());
    let pattern_tail = pattern.rsplit("::").next().unwrap_or(pattern.as_str());
    symbol_tail == pattern_tail || symbol_tail.contains(pattern_tail)
}

fn const_matches(symbol: &str, pattern: &str) -> bool {
    let symbol = normalize_symbol(symbol);
    let pattern = normalize_symbol(pattern);
    symbol == pattern || symbol.contains(&pattern)
}

fn normalize_symbol(value: &str) -> String {
    value
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect::<String>()
        .trim_matches('&')
        .trim_matches('*')
        .to_string()
}

fn scan_workspace(workspace: &CargoWorkspace) -> Result<WorkspaceScan> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_rust::language())
        .context("failed to initialize rust parser for economic checker")?;

    let mut scan = WorkspaceScan::default();
    for member in &workspace.members {
        for file in rust_source_files(&member.path) {
            let Ok(content) = fs::read_to_string(&file) else {
                continue;
            };
            let Some(tree) = parser.parse(&content, None) else {
                continue;
            };

            collect_nodes(
                tree.root_node(),
                &content,
                &member.name,
                &file,
                module_name(&file),
                &mut scan,
            );
        }
    }
    Ok(scan)
}

fn collect_nodes(
    node: Node<'_>,
    content: &str,
    crate_name: &str,
    file: &Path,
    module: String,
    scan: &mut WorkspaceScan,
) {
    match node.kind() {
        "call_expression" => {
            if let Some(function_node) = node
                .child_by_field_name("function")
                .or_else(|| node.child(0))
            {
                let symbol = text_for_node(function_node, content);
                if !symbol.is_empty() {
                    scan.calls.push(SymbolHit {
                        symbol: normalize_symbol(&symbol),
                        location: location_from_node(crate_name, file, &module, content, node),
                    });
                }
            }
        }
        "method_call_expression" => {
            if let Some(method) = node.child_by_field_name("method") {
                let symbol = text_for_node(method, content);
                if !symbol.is_empty() {
                    scan.calls.push(SymbolHit {
                        symbol: normalize_symbol(&symbol),
                        location: location_from_node(crate_name, file, &module, content, node),
                    });
                }
            }
        }
        "struct_expression" => {
            let struct_name = node
                .child_by_field_name("name")
                .map(|name| text_for_node(name, content))
                .unwrap_or_else(|| struct_name_from_expression(&text_for_node(node, content)));
            if !struct_name.is_empty() {
                collect_struct_fields(node, content, crate_name, file, &module, &struct_name, scan);
            }
        }
        "const_item" | "static_item" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                scan.consts.push(ConstHit {
                    name: text_for_node(name_node, content),
                    value: node
                        .child_by_field_name("value")
                        .map(|value| text_for_node(value, content)),
                    location: location_from_node(crate_name, file, &module, content, node),
                });
            }
        }
        _ => {}
    }

    let child_count = node.child_count();
    for idx in 0..child_count {
        if let Some(child) = node.child(idx) {
            collect_nodes(child, content, crate_name, file, module.clone(), scan);
        }
    }
}

fn collect_struct_fields(
    node: Node<'_>,
    content: &str,
    crate_name: &str,
    file: &Path,
    module: &str,
    struct_name: &str,
    scan: &mut WorkspaceScan,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "field_initializer" {
            if let Some(field_node) = child
                .child_by_field_name("field")
                .or_else(|| child.child(0))
            {
                scan.struct_fields.push(StructFieldHit {
                    struct_name: struct_name.to_string(),
                    field: text_for_node(field_node, content),
                    location: location_from_node(crate_name, file, module, content, child),
                });
            }
        }
        collect_struct_fields(child, content, crate_name, file, module, struct_name, scan);
    }
}

fn location_from_node(
    crate_name: &str,
    file: &Path,
    module: &str,
    content: &str,
    node: Node<'_>,
) -> CodeLocation {
    let start = node.start_position().row as u32 + 1;
    let end = node.end_position().row as u32 + 1;
    CodeLocation {
        crate_name: crate_name.to_string(),
        module: module.to_string(),
        file: file.to_path_buf(),
        line_range: (start, end.max(start)),
        snippet: Some(snippet_for_line(content, start, 8)),
    }
}

fn snippet_for_line(content: &str, line: u32, max_lines: u32) -> String {
    let lines = content.lines().collect::<Vec<_>>();
    if lines.is_empty() {
        return String::new();
    }
    let total_lines = lines.len() as u32;
    let mut start = line.saturating_sub(max_lines / 2).max(1);
    let mut end = (start + max_lines - 1).min(total_lines);
    if end - start + 1 < max_lines {
        start = end.saturating_sub(max_lines - 1).max(1);
    }
    if start > end {
        end = start;
    }
    (start..=end)
        .filter_map(|number| lines.get(number as usize - 1))
        .map(|line| (*line).to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

fn rust_source_files(root: &Path) -> Vec<PathBuf> {
    WalkDir::new(root)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.path().to_path_buf())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("rs"))
        .collect()
}

fn module_name(file: &Path) -> String {
    file.file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("unknown")
        .to_string()
}

fn text_for_node(node: Node<'_>, content: &str) -> String {
    content
        .get(node.start_byte()..node.end_byte())
        .unwrap_or_default()
        .to_string()
}

fn struct_name_from_expression(expression: &str) -> String {
    expression
        .split('{')
        .next()
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn default_location(workspace: &CargoWorkspace) -> CodeLocation {
    if let Some(member) = workspace.members.first() {
        return CodeLocation {
            crate_name: member.name.clone(),
            module: "lib".to_string(),
            file: member.path.join("src/lib.rs"),
            line_range: (1, 1),
            snippet: Some("// no direct location captured".to_string()),
        };
    }
    CodeLocation {
        crate_name: "workspace".to_string(),
        module: "root".to_string(),
        file: workspace.root.join("Cargo.toml"),
        line_range: (1, 1),
        snippet: Some("// no direct location captured".to_string()),
    }
}

fn sanitize_prompt_input(text: &str) -> String {
    const MAX_CHARS: usize = 4_000;
    let mut cleaned = String::with_capacity(text.len().min(MAX_CHARS));
    for ch in text.chars() {
        if ch == '\n' || ch == '\t' || !ch.is_control() {
            cleaned.push(ch);
        }
        if cleaned.len() >= MAX_CHARS {
            break;
        }
    }

    let mut out = String::new();
    for line in cleaned.lines() {
        let trimmed = line.trim_start();
        let lower = trimmed.to_ascii_lowercase();
        if lower.starts_with("system:")
            || lower.starts_with("assistant:")
            || lower.starts_with("user:")
        {
            out.push_str("[role-label-redacted]\n");
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    if out.ends_with('\n') {
        out.pop();
    }

    let mut sanitized = out
        .replace("```", "'''")
        .replace("<|", "< ")
        .replace("|>", " >")
        .replace("<<", "< ")
        .replace(">>", " >");

    for marker in [
        "SYSTEM:",
        "System:",
        "system:",
        "ASSISTANT:",
        "Assistant:",
        "assistant:",
        "USER:",
        "User:",
        "user:",
    ] {
        sanitized = sanitized.replace(marker, "[role-label-redacted]:");
    }
    sanitized
}
