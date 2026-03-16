use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use audit_agent_core::workspace::CargoWorkspace;
use engine_crypto::semantic::ra_client::SemanticIndex;
use walkdir::WalkDir;

const LIBP2P_PATTERNS: &[&str] = &[
    "libp2p::",
    "Swarm::",
    "SwarmBuilder",
    "NetworkBehaviour",
    "gossipsub::",
    "kad::",
    "identify::",
];
const TOKIO_RUNTIME_NEW_PATTERNS: &[&str] = &["tokio::runtime::Runtime::new("];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterPoint {
    pub crate_name: String,
    pub file: PathBuf,
    pub line: u32,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BridgeLevel {
    LevelA,
    LevelB { adapter_points: Vec<AdapterPoint> },
    LevelC { reason: String },
}

pub struct MadSimFeasibilityAssessor;

impl MadSimFeasibilityAssessor {
    pub fn assess(workspace: &CargoWorkspace, semantic_index: &SemanticIndex) -> BridgeLevel {
        let runtime_sites = runtime_new_call_sites(workspace);
        let runtime_files = runtime_sites
            .iter()
            .map(|site| site.file.clone())
            .collect::<HashSet<_>>();
        let semantic_runtime_hits = semantic_runtime_new_hits(semantic_index);

        if runtime_files.len() >= 2 || runtime_sites.len() >= 3 || semantic_runtime_hits >= 2 {
            return BridgeLevel::LevelC {
                reason: format!(
                    "scattered tokio::Runtime::new() calls detected at {} source sites across {} files",
                    runtime_sites.len(),
                    runtime_files.len()
                ),
            };
        }

        let mut adapter_points = libp2p_adapter_points(workspace);
        if adapter_points.is_empty() && semantic_has_libp2p_usage(semantic_index) {
            adapter_points.push(AdapterPoint {
                crate_name: "semantic-index".to_string(),
                file: workspace.root.join("Cargo.toml"),
                line: 1,
                reason: "libp2p call detected in semantic index; add explicit adapter mapping"
                    .to_string(),
            });
        }

        if !adapter_points.is_empty() {
            return BridgeLevel::LevelB { adapter_points };
        }

        BridgeLevel::LevelA
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CallSite {
    file: PathBuf,
    line: u32,
}

fn semantic_runtime_new_hits(index: &SemanticIndex) -> usize {
    index
        .call_graph
        .values()
        .flat_map(|callees| callees.iter())
        .filter(|callee| {
            callee.contains("tokio::runtime::Runtime::new")
                || callee.ends_with("Runtime::new")
                || callee == &&"Runtime::new".to_string()
        })
        .count()
}

fn semantic_has_libp2p_usage(index: &SemanticIndex) -> bool {
    index
        .call_graph
        .values()
        .flat_map(|callees| callees.iter())
        .any(|callee| {
            callee.contains("libp2p")
                || callee.contains("Swarm")
                || callee.contains("gossipsub")
                || callee.contains("kad::")
        })
}

fn runtime_new_call_sites(workspace: &CargoWorkspace) -> Vec<CallSite> {
    let mut sites = Vec::new();

    for member in &workspace.members {
        for file in rust_source_files(&member.path) {
            let Ok(content) = fs::read_to_string(&file) else {
                continue;
            };
            for (line_idx, line) in content.lines().enumerate() {
                if TOKIO_RUNTIME_NEW_PATTERNS
                    .iter()
                    .any(|pattern| line.contains(pattern))
                {
                    sites.push(CallSite {
                        file: file.clone(),
                        line: line_idx as u32 + 1,
                    });
                }
            }
        }
    }

    sites
}

fn libp2p_adapter_points(workspace: &CargoWorkspace) -> Vec<AdapterPoint> {
    let mut points = Vec::new();
    let mut seen = HashSet::<(String, PathBuf, u32)>::new();

    for member in &workspace.members {
        for file in rust_source_files(&member.path) {
            let Ok(content) = fs::read_to_string(&file) else {
                continue;
            };
            for (line_idx, line) in content.lines().enumerate() {
                let Some(pattern) = LIBP2P_PATTERNS
                    .iter()
                    .find(|pattern| line.contains(*pattern))
                else {
                    continue;
                };

                let key = (member.name.clone(), file.clone(), line_idx as u32 + 1);
                if !seen.insert(key) {
                    continue;
                }

                points.push(AdapterPoint {
                    crate_name: member.name.clone(),
                    file: file.clone(),
                    line: line_idx as u32 + 1,
                    reason: format!("libp2p adapter candidate from `{pattern}`"),
                });
            }
        }
    }

    points.sort_by(|a, b| {
        a.crate_name
            .cmp(&b.crate_name)
            .then(a.file.cmp(&b.file))
            .then(a.line.cmp(&b.line))
    });
    points
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
