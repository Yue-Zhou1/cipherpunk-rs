use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;

use anyhow::Result;
use audit_agent_core::finding::Severity;
use audit_agent_core::workspace::CargoWorkspace;
use regex::Regex;
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
    let fn_def_re = Regex::new(r"^\s*(?:pub\s+)?(?:async\s+)?fn\s+([a-zA-Z_][a-zA-Z0-9_]*)").expect("fn regex");
    let call_re = Regex::new(r"\b([a-zA-Z_][a-zA-Z0-9_]*)\s*\(").expect("call regex");
    let excluded = HashSet::from([
        "if", "for", "while", "loop", "match", "return", "Some", "None", "Ok", "Err",
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

            let mut current_fn: Option<String> = None;
            let mut brace_depth: i32 = 0;
            for line in content.lines() {
                if let Some(captures) = fn_def_re.captures(line) {
                    let name = captures[1].to_string();
                    current_fn = Some(name.clone());
                    edges.entry(name).or_default();
                    brace_depth = 0;
                }

                if let Some(caller) = current_fn.as_ref() {
                    for cap in call_re.captures_iter(line) {
                        let callee = cap[1].to_string();
                        if excluded.contains(callee.as_str()) || callee == *caller {
                            continue;
                        }
                        edges.entry(caller.clone()).or_default().insert(callee);
                    }
                }

                brace_depth += line.matches('{').count() as i32;
                brace_depth -= line.matches('}').count() as i32;
                if brace_depth <= 0 && line.contains('}') {
                    current_fn = None;
                }
            }
        }
    }

    Ok(edges)
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
