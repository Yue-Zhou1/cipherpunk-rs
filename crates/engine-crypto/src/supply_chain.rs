use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;

use anyhow::Result;
use audit_agent_core::finding::Severity;
use audit_agent_core::workspace::CargoWorkspace;
use tree_sitter::Parser;
use walkdir::WalkDir;

pub struct SupplyChainAnalyzer {
    /// Phase 1: TreeSitterCallGraph (name-based, no cross-crate resolution)
    /// Phase 3+: SemanticCallGraph (rust-analyzer backed, full resolution)
    call_graph: CallGraphBackend,
    advisories: Vec<CargoAuditAdvisory>,
}

pub enum CallGraphBackend {
    TreeSitter(TreeSitterCallGraph),
    Semantic(SemanticCallGraph),
}

pub struct TreeSitterCallGraph;

pub struct SemanticCallGraph;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CveCallPathResult {
    pub cve_id: String,
    pub crate_name: String,
    pub affected_fn: String,
    pub reachable_from_crypto_path: bool,
    pub call_chain: Vec<String>,
    pub original_severity: Severity,
    pub adjusted_severity: Severity,
    pub adjustment_reason: String,
    pub graph_backend: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CargoAuditAdvisory {
    pub cve_id: String,
    pub crate_name: String,
    pub affected_fn: String,
    pub severity: Severity,
    pub dependency_kind: DependencyKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DependencyKind {
    Normal,
    Dev,
}

impl SupplyChainAnalyzer {
    pub fn tree_sitter(advisories: Vec<CargoAuditAdvisory>) -> Self {
        Self {
            call_graph: CallGraphBackend::TreeSitter(TreeSitterCallGraph),
            advisories,
        }
    }

    pub async fn analyze(&self, workspace: &CargoWorkspace) -> Result<Vec<CveCallPathResult>> {
        let (edges, entry_points) = match &self.call_graph {
            CallGraphBackend::TreeSitter(_) => {
                let graph = build_tree_sitter_call_graph(workspace)?;
                let entries = detect_crypto_entries(&graph);
                (graph, entries)
            }
            CallGraphBackend::Semantic(_) => (HashMap::new(), HashSet::new()),
        };

        let mut results = Vec::new();
        for advisory in &self.advisories {
            let mut result = CveCallPathResult {
                cve_id: advisory.cve_id.clone(),
                crate_name: advisory.crate_name.clone(),
                affected_fn: advisory.affected_fn.clone(),
                reachable_from_crypto_path: false,
                call_chain: vec![],
                original_severity: advisory.severity.clone(),
                adjusted_severity: advisory.severity.clone(),
                adjustment_reason: "no escalation".to_string(),
                graph_backend: match self.call_graph {
                    CallGraphBackend::TreeSitter(_) => "tree-sitter".to_string(),
                    CallGraphBackend::Semantic(_) => "semantic".to_string(),
                },
            };

            if advisory.dependency_kind == DependencyKind::Dev {
                result.adjusted_severity = Severity::Low;
                result.adjustment_reason =
                    "dev-dependency advisory downgraded for runtime impact".to_string();
                results.push(result);
                continue;
            }

            if let Some(call_chain) = find_reachable_chain(&edges, &entry_points, &advisory.affected_fn)
            {
                result.reachable_from_crypto_path = true;
                result.call_chain = call_chain.clone();
                let depth = call_chain.len().saturating_sub(1);
                if depth <= 3 {
                    result.adjusted_severity = Severity::Critical;
                    result.adjustment_reason =
                        "affected function reachable within <=3 frames from crypto entry point"
                            .to_string();
                } else {
                    result.adjusted_severity = max_severity(advisory.severity.clone(), Severity::High);
                    result.adjustment_reason =
                        "affected function reachable from crypto entry point".to_string();
                }
            }

            results.push(result);
        }

        Ok(results)
    }
}

fn build_tree_sitter_call_graph(workspace: &CargoWorkspace) -> Result<HashMap<String, HashSet<String>>> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_rust::language())
        .map_err(|e| anyhow::anyhow!("failed to set language: {e}"))?;

    let excluded = HashSet::from([
        "if", "for", "while", "loop", "match", "return", "Some", "None", "Ok", "Err",
        "format", "println", "eprintln", "vec", "assert", "assert_eq", "assert_ne",
        "panic", "todo", "unimplemented", "unreachable",
    ]);

    let mut edges: HashMap<String, HashSet<String>> = HashMap::new();

    for member in &workspace.members {
        for entry in WalkDir::new(&member.path).into_iter().filter_map(|e| e.ok()) {
            if !entry.file_type().is_file() {
                continue;
            }
            if entry.path().extension().and_then(|ext| ext.to_str()) != Some("rs") {
                continue;
            }
            let content = fs::read_to_string(entry.path())?;
            let Some(tree) = parser.parse(&content, None) else {
                continue;
            };

            collect_call_edges(&content, tree.root_node(), &excluded, &mut edges);
        }
    }

    Ok(edges)
}

fn collect_call_edges(
    content: &str,
    node: tree_sitter::Node,
    excluded: &HashSet<&str>,
    edges: &mut HashMap<String, HashSet<String>>,
) {
    // Find function definitions
    if node.kind() == "function_item" {
        if let Some(name_node) = node.child_by_field_name("name") {
            let fn_name = &content[name_node.start_byte()..name_node.end_byte()];
            edges.entry(fn_name.to_string()).or_default();

            // Find all call_expression nodes inside this function's body
            if let Some(body) = node.child_by_field_name("body") {
                collect_calls_in_body(content, body, fn_name, excluded, edges);
            }
        }
    }

    // Recurse into children (but not into nested function bodies — those are handled above)
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            if child.kind() != "function_item" || node.kind() != "function_item" {
                collect_call_edges(content, child, excluded, edges);
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

fn collect_calls_in_body(
    content: &str,
    node: tree_sitter::Node,
    caller: &str,
    excluded: &HashSet<&str>,
    edges: &mut HashMap<String, HashSet<String>>,
) {
    if node.kind() == "call_expression" {
        if let Some(fn_node) = node.child(0) {
            let fn_text = &content[fn_node.start_byte()..fn_node.end_byte()];
            // Get the final segment of any path: "module::func" -> "func"
            let callee = fn_text.rsplit("::").next().unwrap_or(fn_text);
            if !excluded.contains(callee) && callee != caller {
                edges
                    .entry(caller.to_string())
                    .or_default()
                    .insert(callee.to_string());
            }
        }
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            collect_calls_in_body(content, cursor.node(), caller, excluded, edges);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

fn detect_crypto_entries(edges: &HashMap<String, HashSet<String>>) -> HashSet<String> {
    edges
        .keys()
        .filter(|name| {
            let lower = name.to_ascii_lowercase();
            lower.contains("verify")
                || lower.contains("prove")
                || lower.contains("sign")
                || lower.contains("keygen")
                || lower.contains("ingest")
        })
        .cloned()
        .collect()
}

fn find_reachable_chain(
    edges: &HashMap<String, HashSet<String>>,
    entries: &HashSet<String>,
    target: &str,
) -> Option<Vec<String>> {
    let mut queue = VecDeque::<Vec<String>>::new();
    let mut visited = HashSet::<String>::new();

    for entry in entries {
        queue.push_back(vec![entry.clone()]);
        visited.insert(entry.clone());
    }

    while let Some(path) = queue.pop_front() {
        let node = path.last()?;
        if node == target {
            return Some(path);
        }

        for next in edges.get(node).into_iter().flat_map(|s| s.iter()) {
            if visited.insert(next.clone()) {
                let mut next_path = path.clone();
                next_path.push(next.clone());
                queue.push_back(next_path);
            }
        }
    }

    None
}

fn max_severity(a: Severity, b: Severity) -> Severity {
    if severity_rank(&a) >= severity_rank(&b) {
        a
    } else {
        b
    }
}

fn severity_rank(severity: &Severity) -> u8 {
    match severity {
        Severity::Critical => 5,
        Severity::High => 4,
        Severity::Medium => 3,
        Severity::Low => 2,
        Severity::Observation => 1,
    }
}

/// Parse the JSON output of `cargo audit --json` into advisories.
pub fn parse_cargo_audit_json(output: &str) -> Result<Vec<CargoAuditAdvisory>> {
    // cargo audit --json produces: { "vulnerabilities": { "list": [ { "advisory": { "id": "...", ... }, "versions": { ... }, "affected": { "functions": { "crate::fn": [...] } } } ] } }
    let parsed: serde_json::Value = serde_json::from_str(output)?;
    let mut advisories = Vec::new();

    let empty_list = vec![];
    let vuln_list = parsed
        .get("vulnerabilities")
        .and_then(|v| v.get("list"))
        .and_then(|v| v.as_array())
        .unwrap_or(&empty_list);

    for vuln in vuln_list {
        let advisory = vuln.get("advisory");
        let cve_id = advisory
            .and_then(|a| a.get("id"))
            .and_then(|v| v.as_str())
            .unwrap_or("UNKNOWN")
            .to_string();
        let crate_name = advisory
            .and_then(|a| a.get("package"))
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        // Extract affected function names from the "affected.functions" map
        let affected_fns: Vec<String> = vuln
            .get("affected")
            .and_then(|a| a.get("functions"))
            .and_then(|f| f.as_object())
            .map(|map| map.keys().cloned().collect())
            .unwrap_or_default();

        let affected_fn = affected_fns.first().cloned().unwrap_or_default();

        advisories.push(CargoAuditAdvisory {
            cve_id,
            crate_name,
            affected_fn,
            severity: Severity::Medium, // default; caller can adjust based on CVSS
            dependency_kind: DependencyKind::Normal,
        });
    }

    Ok(advisories)
}
