# Explorer API Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace fixture data in the CodebaseExplorer Graph tab with real backend data via a dedicated Axum endpoint, using on-demand hierarchical loading.

**Architecture:** New `ExplorerGraphBuilder` module in `session-manager` crate merges all four `ProjectIr` graphs into a hierarchy with deterministic hash IDs. A new Axum endpoint serves overview/cluster/full responses. Frontend `useUnifiedGraph` hook is rewritten from fixture-loading to API-calling with on-demand cluster expansion. Old graph commands, transport routes, fixture data, and fallback functions are removed.

**Tech Stack:** Rust (Axum, serde), TypeScript (React 18, ReactFlow 11), Vitest, Testing Library

**Spec:** `docs/superpowers/specs/2026-03-28-explorer-api-integration-design.md`

---

## File Structure

### New Files

| File | Responsibility |
|------|----------------|
| `crates/services/session-manager/src/explorer_graph.rs` | `ExplorerGraphBuilder`, response types, `hash_id()`, `ExplorerDepth` enum |
| `crates/services/session-manager/src/explorer_graph_tests.rs` | Unit tests for the builder (ID determinism, hierarchy, depth filtering, signatures, edge merging) |

### Modified Files

| File | Change |
|------|--------|
| `crates/services/session-manager/src/lib.rs` | Add `mod explorer_graph;` declaration |
| `crates/services/session-manager/src/state.rs` | Add `load_explorer_graph()` method to `SessionManager`, add `explorer_graph_stale` event emission |
| `crates/apps/web-server/src/lib.rs` | Register new `/api/sessions/:session_id/explorer-graph` route, add handler |
| `ui/src/ipc/transport.ts` | Add `load_explorer_graph` route, remove old graph routes/timeout constants |
| `ui/src/ipc/commands.ts` | Add `loadExplorerGraph()` + response types, remove old graph functions/types |
| `ui/src/ipc/commands.fixtures.ts` | Remove old graph fallback functions |
| `ui/src/features/workstation/CodebaseExplorer/types.ts` | Add `childCount` to `ExplorerNode`, add new fields to `ExplorerContextValue` |
| `ui/src/features/workstation/CodebaseExplorer/hooks/useUnifiedGraph.ts` | Full rewrite: fixture loading → API loading with on-demand expansion |
| `ui/src/features/workstation/CodebaseExplorer/AdaptiveLayout.tsx` | Replace ID-parsing functions with `contains`-edge parentMap |
| `ui/src/features/workstation/CodebaseExplorer/ExplorerContext.tsx` | Accept `sessionId`, wire new hook fields, expose `nodeMap`/loading/error/stale |
| `ui/src/features/workstation/CodebaseExplorer/index.tsx` | Pass `sessionId` through, add loading/error/stale UI |
| `ui/src/features/workstation/CodebaseExplorer/nodes/ClusterNode.tsx` | Add loading spinner when cluster is being fetched |
| `ui/src/features/workstation/CodebaseExplorer/ExplorerCanvas.tsx` | Use `expandCluster()` on cluster click |
| `ui/src/styles.css` | Add CSS for stale banner, loading states, error states |
| `ui/src/features/workstation/CodebaseExplorer/__tests__/hooks.test.ts` | Remove fixture tests, rewrite with inline data + mocked API |
| `ui/src/features/workstation/CodebaseExplorer/__tests__/CodebaseExplorer.test.tsx` | Rewrite to mock `loadExplorerGraph`, test loading/error/stale states |

### Deleted Files

| File | Deleted In | Reason |
|------|-----------|--------|
| `ui/src/features/workstation/CodebaseExplorer/fixtures/mockGraph.ts` | Task 7 | Replaced by backend API (deleted after useUnifiedGraph rewrite removes its last import) |

---

## TDD Guidance

**Backend (Tasks 1-3):** Task 1 creates types + hash function. Task 2 implements the builder. Task 3 writes tests and verifies they pass. Ideally, implementers should write each test before the corresponding builder method (red-green within Task 2/3), but the tasks are ordered implementation-first for readability. The executing agent should interleave: write a test, make it fail, implement, make it pass.

**Frontend (Tasks 7-13):** Tasks 12-13 (test rewrites) should be done **after** the corresponding implementation tasks. For each implementation task, run the existing test suite to catch regressions early.

## Task Dependency Graph

```
1 → 2 → 3 → 4
                ↘
5 (independent)  → 6 → 7 → 8 → 9 → 10 → 11 → 12 → 13
```

Tasks 1-4 are backend. Task 5 is frontend cleanup (can run parallel with backend). Tasks 6-13 are frontend integration (sequential chain).

---

## Task 1: Backend Response Types and Hash ID

**Files:**
- Create: `crates/services/session-manager/src/explorer_graph.rs`
- Modify: `crates/services/session-manager/src/lib.rs`

- [ ] **Step 1: Create the explorer_graph module with response types and hash function**

```rust
// crates/services/session-manager/src/explorer_graph.rs

use serde::Serialize;
use std::path::Path;

use project_ir::ProjectIr;

// ── Response types ──────────────────────────────────────────────

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

// ── Depth control ───────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExplorerDepth {
    Overview,
    Full,
}

// ── Deterministic hash ID ───────────────────────────────────────

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

#[cfg(test)]
#[path = "explorer_graph_tests.rs"]
mod tests;
```

- [ ] **Step 2: Register the module in lib.rs**

Add `mod explorer_graph;` to `crates/services/session-manager/src/lib.rs` alongside existing module declarations. Also add `pub use explorer_graph::*;` to re-export the types.

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p session-manager`
Expected: compiles with no errors (unused import warnings are OK at this stage)

- [ ] **Step 4: Commit**

```bash
git add crates/services/session-manager/src/explorer_graph.rs crates/services/session-manager/src/lib.rs
git commit -m "feat(explorer-api): add response types, ExplorerDepth, and hash_id function"
```

---

## Task 2: Backend ExplorerGraphBuilder

**Files:**
- Modify: `crates/services/session-manager/src/explorer_graph.rs`

**Reference:** Read `crates/data/project-ir/src/graph.rs` for `ProjectIr`, `SymbolNode`, `FunctionSignature`, `ParameterInfo`, `FileNode`, `BasicEdge`, `DataflowEdge` struct definitions.

- [ ] **Step 1: Add the ExplorerGraphBuilder struct and build method**

Append to `explorer_graph.rs`, after the `hash_id` function but before `#[cfg(test)]`:

```rust
use std::collections::{HashMap, HashSet};

// ── Builder ─────────────────────────────────────────────────────

pub struct ExplorerGraphBuilder<'a> {
    ir: &'a ProjectIr,
    root: &'a Path,
}

impl<'a> ExplorerGraphBuilder<'a> {
    pub fn new(ir: &'a ProjectIr, root: &'a Path) -> Self {
        Self { ir, root }
    }

    /// Build the full explorer graph, then filter by depth/cluster.
    pub fn build(
        &self,
        session_id: &str,
        depth: ExplorerDepth,
        cluster: Option<&str>,
    ) -> Result<ExplorerGraphResponse, ExplorerBuildError> {
        // Step 1-3: Build hierarchy nodes and contains edges
        let (mut nodes, mut edges) = self.build_hierarchy();

        // Step 4-5: Add symbols and cross-file edges (skip for overview)
        let is_overview = cluster.is_none() && depth == ExplorerDepth::Overview;
        if !is_overview {
            self.add_symbols(&mut nodes, &mut edges);
            self.add_cross_file_edges(&mut edges, &nodes);
        }

        // Step 6: Generate hash IDs (replace temp IDs with hashed ones)
        let id_map = self.assign_hash_ids(&mut nodes, &mut edges);

        // Step 7: Attach child counts
        self.attach_child_counts(&mut nodes, &edges);

        // Filter by cluster if requested
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

    /// Steps 1-3: Detect crates, infer modules, build crate→module→file hierarchy.
    fn build_hierarchy(&self) -> (Vec<ExplorerNodeResponse>, Vec<ExplorerEdgeResponse>) {
        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        let mut seen_crates: HashMap<String, String> = HashMap::new(); // crate_name → temp_id
        let mut seen_modules: HashMap<String, String> = HashMap::new(); // module_path → temp_id

        for file_node in &self.ir.file_graph.nodes {
            let rel_path = file_node
                .path
                .strip_prefix(self.root)
                .unwrap_or(&file_node.path);
            let rel_str = rel_path.to_string_lossy().replace('\\', "/");
            let segments: Vec<&str> = rel_str.split('/').collect();

            if segments.is_empty() {
                continue;
            }

            // Infer crate name from first path segment
            let crate_name = segments[0].to_string();
            let crate_temp_id = format!("__crate:{}", crate_name);
            if !seen_crates.contains_key(&crate_name) {
                seen_crates.insert(crate_name.clone(), crate_temp_id.clone());
                nodes.push(ExplorerNodeResponse {
                    id: crate_temp_id.clone(),
                    label: crate_name.clone(),
                    kind: "crate".to_string(),
                    file_path: None,
                    line: None,
                    signature: None,
                    child_count: None,
                });
            }

            // Infer module from directory path (all segments except filename)
            if segments.len() > 2 {
                // e.g. "engine-crypto/src/verify.rs" → module "engine-crypto/src"
                let dir_path = segments[..segments.len() - 1].join("/");
                let module_temp_id = format!("__module:{}", dir_path);
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
                    // module → crate contains edge
                    edges.push(ExplorerEdgeResponse {
                        from: crate_temp_id.clone(),
                        to: module_temp_id.clone(),
                        relation: "contains".to_string(),
                        parameter_name: None,
                        parameter_position: None,
                        value_preview: None,
                    });
                }

                // file node
                let file_temp_id = format!("__file:{}", rel_str);
                let file_label = segments.last().unwrap_or(&"").to_string();
                nodes.push(ExplorerNodeResponse {
                    id: file_temp_id.clone(),
                    label: file_label,
                    kind: "file".to_string(),
                    file_path: Some(rel_str.clone()),
                    line: None,
                    signature: None,
                    child_count: None,
                });
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
            } else if segments.len() == 2 {
                // File directly under crate root (e.g. "intake/lib.rs")
                let file_temp_id = format!("__file:{}", rel_str);
                let file_label = segments[1].to_string();
                nodes.push(ExplorerNodeResponse {
                    id: file_temp_id.clone(),
                    label: file_label,
                    kind: "file".to_string(),
                    file_path: Some(rel_str.clone()),
                    line: None,
                    signature: None,
                    child_count: None,
                });
                edges.push(ExplorerEdgeResponse {
                    from: crate_temp_id.clone(),
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

    /// Step 4: Add symbol nodes from symbol_graph, with signatures.
    fn add_symbols(
        &self,
        nodes: &mut Vec<ExplorerNodeResponse>,
        edges: &mut Vec<ExplorerEdgeResponse>,
    ) {
        for sym in &self.ir.symbol_graph.nodes {
            let rel_path = sym
                .file
                .strip_prefix(self.root)
                .unwrap_or(&sym.file);
            let rel_str = rel_path.to_string_lossy().replace('\\', "/");
            let sym_temp_id = format!("__symbol:{}::{}", rel_str, sym.name);
            let file_temp_id = format!("__file:{}", rel_str);

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

            // file → symbol contains edge
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

    /// Step 5: Add cross-file edges (calls, parameter_flow, return_flow) from
    /// symbol_graph.edges and dataflow_graph.edges.
    fn add_cross_file_edges(
        &self,
        edges: &mut Vec<ExplorerEdgeResponse>,
        nodes: &[ExplorerNodeResponse],
    ) {
        // Build a lookup from original symbol ID → our temp ID
        let sym_id_map: HashMap<String, String> = self
            .ir
            .symbol_graph
            .nodes
            .iter()
            .map(|sym| {
                let rel_path = sym
                    .file
                    .strip_prefix(self.root)
                    .unwrap_or(&sym.file);
                let rel_str = rel_path.to_string_lossy().replace('\\', "/");
                (sym.id.clone(), format!("__symbol:{}::{}", rel_str, sym.name))
            })
            .collect();

        // Symbol graph edges (calls)
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

        // Dataflow graph edges → parameter_flow / return_flow
        let df_id_map: HashMap<String, String> = self
            .ir
            .dataflow_graph
            .nodes
            .iter()
            .filter_map(|df_node| {
                // Try to match dataflow nodes to symbol nodes by label
                sym_id_map
                    .iter()
                    .find(|(orig_id, _)| orig_id == &df_node.id)
                    .map(|(_, temp_id)| (df_node.id.clone(), temp_id.clone()))
            })
            .collect();

        for edge in &self.ir.dataflow_graph.edges {
            if let (Some(from_id), Some(to_id)) =
                (df_id_map.get(&edge.from), df_id_map.get(&edge.to))
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

    /// Step 6: Replace all temp IDs (__crate:X, __module:X, etc.) with hash IDs.
    fn assign_hash_ids(
        &self,
        nodes: &mut Vec<ExplorerNodeResponse>,
        edges: &mut Vec<ExplorerEdgeResponse>,
    ) -> HashMap<String, String> {
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

        id_map
    }

    /// Step 7: Count direct children per cluster node.
    fn attach_child_counts(
        &self,
        nodes: &mut Vec<ExplorerNodeResponse>,
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
                node.child_count = counts.get(&node.id).copied();
            }
        }
    }

    /// Filter nodes/edges to only the descendants of a specific cluster.
    /// Uses BFS to collect all descendants at any depth (not just 2 levels).
    fn filter_to_cluster(
        &self,
        nodes: &mut Vec<ExplorerNodeResponse>,
        edges: &mut Vec<ExplorerEdgeResponse>,
        cluster_id: &str,
    ) {
        use std::collections::VecDeque;

        // Build a children lookup from contains edges
        let mut children_of: HashMap<&str, Vec<&str>> = HashMap::new();
        for edge in edges.iter() {
            if edge.relation == "contains" {
                children_of
                    .entry(edge.from.as_str())
                    .or_default()
                    .push(edge.to.as_str());
            }
        }

        // BFS to collect all descendants
        let mut all_descendant_ids: HashSet<String> = HashSet::new();
        let mut queue: VecDeque<&str> = VecDeque::new();
        queue.push_back(cluster_id);
        while let Some(current) = queue.pop_front() {
            if let Some(children) = children_of.get(current) {
                for &child in children {
                    if all_descendant_ids.insert(child.to_string()) {
                        queue.push_back(child);
                    }
                }
            }
        }

        // Keep only descendant nodes
        nodes.retain(|n| all_descendant_ids.contains(&n.id));

        // Keep edges where both endpoints are in the descendant set
        edges.retain(|e| {
            all_descendant_ids.contains(&e.from) && all_descendant_ids.contains(&e.to)
        });
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ExplorerBuildError {
    #[error("Unknown cluster node ID: {0}")]
    UnknownCluster(String),
}
```

- [ ] **Step 2: Add required use statements at the top of the file**

Ensure `std::collections::{HashMap, HashSet}` is imported. Also ensure the `ProjectIr` import path is correct based on the crate's dependency on `project-ir`.

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p session-manager`
Expected: compiles with no errors

- [ ] **Step 4: Commit**

```bash
git add crates/services/session-manager/src/explorer_graph.rs
git commit -m "feat(explorer-api): implement ExplorerGraphBuilder with hierarchy, signatures, and hash IDs"
```

---

## Task 3: Backend Builder Tests

**Files:**
- Create: `crates/services/session-manager/src/explorer_graph_tests.rs`

**Reference:** Read `crates/data/project-ir/src/graph.rs` for how to construct test `ProjectIr`, `SymbolNode`, `FileNode`, `BasicEdge`, `DataflowEdge` instances.

- [ ] **Step 1: Write unit tests for hash_id determinism**

```rust
// crates/services/session-manager/src/explorer_graph_tests.rs

use super::*;
use std::path::PathBuf;
use project_ir::graph::*;

#[test]
fn hash_id_is_deterministic() {
    let id1 = hash_id("sym", "engine-crypto/src/verify.rs::verify_signature");
    let id2 = hash_id("sym", "engine-crypto/src/verify.rs::verify_signature");
    assert_eq!(id1, id2);
    assert!(id1.starts_with("sym_"));
    assert_eq!(id1.len(), 16); // "sym_" (4) + 12 hex chars
}

#[test]
fn hash_id_different_inputs_differ() {
    let id1 = hash_id("sym", "a.rs::foo");
    let id2 = hash_id("sym", "a.rs::bar");
    assert_ne!(id1, id2);
}

#[test]
fn hash_id_different_prefixes_differ() {
    let id1 = hash_id("crt", "engine-crypto");
    let id2 = hash_id("mod", "engine-crypto");
    assert_ne!(id1, id2);
}
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test -p session-manager -- explorer_graph`
Expected: 3 tests pass

- [ ] **Step 3: Write test for hierarchy construction with a minimal ProjectIr**

```rust
fn make_test_ir(root: &Path) -> ProjectIr {
    let file_nodes = vec![
        FileNode {
            id: "file:mycrate/src/lib.rs".into(),
            path: root.join("mycrate/src/lib.rs"),
            language: "rust".into(),
        },
        FileNode {
            id: "file:mycrate/src/utils.rs".into(),
            path: root.join("mycrate/src/utils.rs"),
            language: "rust".into(),
        },
    ];
    let sym_nodes = vec![SymbolNode {
        id: "sym:mycrate/src/lib.rs::do_thing".into(),
        name: "do_thing".into(),
        qualified_name: Some("mycrate::do_thing".into()),
        file: root.join("mycrate/src/lib.rs"),
        kind: "function".into(),
        line: 10,
        signature: Some(FunctionSignature {
            parameters: vec![ParameterInfo {
                name: "input".into(),
                type_annotation: Some("&[u8]".into()),
                position: 0,
            }],
            return_type: Some("bool".into()),
        }),
    }];
    ProjectIr {
        file_graph: Graph {
            nodes: file_nodes,
            edges: vec![],
        },
        symbol_graph: Graph {
            nodes: sym_nodes,
            edges: vec![],
        },
        feature_graph: Graph::default(),
        dataflow_graph: Graph::default(),
        framework_views: vec![],
    }
}

#[test]
fn overview_returns_hierarchy_without_symbols() {
    let root = PathBuf::from("/project");
    let ir = make_test_ir(&root);
    let builder = ExplorerGraphBuilder::new(&ir, &root);
    let resp = builder.build("sess-1", ExplorerDepth::Overview, None).unwrap();

    // Should have: 1 crate + 1 module + 2 files = 4 nodes
    assert_eq!(resp.nodes.len(), 4);

    // No symbol nodes in overview
    assert!(resp.nodes.iter().all(|n| n.kind != "function"));

    // All cluster nodes should have child_count set
    let crate_node = resp.nodes.iter().find(|n| n.kind == "crate").unwrap();
    assert!(crate_node.child_count.is_some());
    assert!(crate_node.id.starts_with("crt_"));
}

#[test]
fn full_depth_includes_symbols_with_signatures() {
    let root = PathBuf::from("/project");
    let ir = make_test_ir(&root);
    let builder = ExplorerGraphBuilder::new(&ir, &root);
    let resp = builder.build("sess-1", ExplorerDepth::Full, None).unwrap();

    // Should have: 1 crate + 1 module + 2 files + 1 symbol = 5 nodes
    assert_eq!(resp.nodes.len(), 5);

    let sym = resp.nodes.iter().find(|n| n.kind == "function").unwrap();
    assert_eq!(sym.label, "do_thing");
    assert!(sym.id.starts_with("sym_"));
    assert!(sym.signature.is_some());
    let sig = sym.signature.as_ref().unwrap();
    assert_eq!(sig.parameters.len(), 1);
    assert_eq!(sig.parameters[0].name, "input");
    assert_eq!(sig.return_type.as_deref(), Some("bool"));
}

#[test]
fn cluster_filter_returns_only_children() {
    let root = PathBuf::from("/project");
    let ir = make_test_ir(&root);
    let builder = ExplorerGraphBuilder::new(&ir, &root);

    // First get the full graph to find the crate node ID
    let full = builder.build("sess-1", ExplorerDepth::Full, None).unwrap();
    let crate_id = full.nodes.iter().find(|n| n.kind == "crate").unwrap().id.clone();

    // Now request just this cluster's children
    let filtered = builder.build("sess-1", ExplorerDepth::Full, Some(&crate_id)).unwrap();

    // Should NOT include the crate node itself
    assert!(filtered.nodes.iter().all(|n| n.id != crate_id));
    // Should include children (module, files, symbols)
    assert!(!filtered.nodes.is_empty());
}

#[test]
fn unknown_cluster_returns_error() {
    let root = PathBuf::from("/project");
    let ir = make_test_ir(&root);
    let builder = ExplorerGraphBuilder::new(&ir, &root);
    let result = builder.build("sess-1", ExplorerDepth::Full, Some("nonexistent_id"));
    assert!(result.is_err());
}
```

- [ ] **Step 4: Run all builder tests**

Run: `cargo test -p session-manager -- explorer_graph`
Expected: 7 tests pass

- [ ] **Step 5: Commit**

```bash
git add crates/services/session-manager/src/explorer_graph_tests.rs
git commit -m "test(explorer-api): add unit tests for ExplorerGraphBuilder"
```

---

## Task 4: Backend Axum Route and Session Manager Method

**Files:**
- Modify: `crates/apps/web-server/src/lib.rs` (add route at ~line 236, add handler after line 543)
- Modify: `crates/services/session-manager/src/state.rs` (add `load_explorer_graph` method after line 961)

- [ ] **Step 1: Add the route registration**

In `crates/apps/web-server/src/lib.rs`, near line 236 (alongside existing graph route), add:

```rust
.route("/api/sessions/:session_id/explorer-graph", get(load_explorer_graph))
```

- [ ] **Step 2: Add the handler function**

After the existing `load_graph` handler (~line 543), add:

```rust
#[derive(Debug, Deserialize)]
pub struct ExplorerGraphQuery {
    #[serde(default = "default_explorer_depth")]
    depth: String,
    cluster: Option<String>,
}

fn default_explorer_depth() -> String {
    "overview".to_string()
}

async fn load_explorer_graph(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Query(query): Query<ExplorerGraphQuery>,
) -> Result<Json<ExplorerGraphResponse>, AppError> {
    let depth = match query.depth.as_str() {
        "full" => ExplorerDepth::Full,
        _ => ExplorerDepth::Overview,
    };

    // Validate: depth=full + cluster is not allowed
    if depth == ExplorerDepth::Full && query.cluster.is_some() {
        return Err(AppError::bad_request(
            "Cannot specify both depth=full and cluster parameter",
        ));
    }

    let response = state
        .manager
        .load_explorer_graph(&session_id, depth, query.cluster.as_deref())
        .await?;

    Ok(Json(response))
}
```

- [ ] **Step 3: Add the session manager method on `UiSessionState`**

In `crates/services/session-manager/src/state.rs`, after the existing `load_symbol_graph` method (~line 961), add the method on `UiSessionState` (following the exact same pattern as `load_symbol_graph` above it):

```rust
pub async fn load_explorer_graph(
    &mut self,
    session_id: &str,
    depth: ExplorerDepth,
    cluster: Option<&str>,
) -> Result<ExplorerGraphResponse> {
    let source_root = self
        .ensure_session_loaded(session_id)?
        .snapshot
        .source
        .local_path
        .clone();
    let ir = self
        .load_or_build_project_ir(session_id, false)
        .await
        .map_err(map_project_ir_build_error)?;
    let builder = ExplorerGraphBuilder::new(&ir, &source_root);
    builder
        .build(session_id, depth, cluster)
        .map_err(|e| anyhow::anyhow!("{}", e))
}
```

Then in `crates/services/session-manager/src/lib.rs`, add the delegating method on `SessionManager` (following the pattern of existing delegating methods):

```rust
pub async fn load_explorer_graph(
    &self,
    session_id: &str,
    depth: ExplorerDepth,
    cluster: Option<&str>,
) -> SessionResult<ExplorerGraphResponse> {
    let mut state = self.inner.lock().await;
    state
        .load_explorer_graph(session_id, depth, cluster)
        .await
        .map_err(map_state_error)
}
```

- [ ] **Step 4: Add required imports**

In `state.rs`, add: `use crate::explorer_graph::{ExplorerGraphBuilder, ExplorerGraphResponse, ExplorerDepth};`

In `lib.rs`, add the import for `ExplorerGraphResponse`, `ExplorerDepth`, and `ExplorerGraphQuery` from the session-manager crate.

- [ ] **Step 5: Verify it compiles**

Run: `cargo check -p web-server`
Expected: compiles (may have warnings about unused items — that's OK)

- [ ] **Step 6: Commit**

```bash
git add crates/apps/web-server/src/lib.rs crates/services/session-manager/src/state.rs
git commit -m "feat(explorer-api): add Axum route and session manager method for explorer-graph endpoint"
```

---

## Task 5: Frontend Cleanup — Remove Old Graph Code

**Files:**
- Modify: `ui/src/ipc/transport.ts` (remove lines 136-157, 212-218, 293-312)
- Modify: `ui/src/ipc/commands.ts` (remove lines 157, 365-389, 479-506)
- Modify: `ui/src/ipc/commands.fixtures.ts` (remove lines 585-605 and fixture constants 149-267)

- [ ] **Step 1: Remove old graph routes from transport.ts**

In `ui/src/ipc/transport.ts`:
1. Remove `load_file_graph`, `load_feature_graph`, `load_dataflow_graph`, `load_symbol_graph` entries from `COMMAND_ROUTES` (lines 136-157)
2. Remove `GRAPH_TIMEOUT_MS` constant (line 212)
3. Remove `GRAPH_COMMANDS` set (lines 213-218)
4. Remove the special-case timeout logic in `HttpTransport.invoke()` that checks `GRAPH_COMMANDS.has(command)` (lines 293-298 and the catch block at 310-312)

- [ ] **Step 2: Remove old graph functions and types from commands.ts**

In `ui/src/ipc/commands.ts`:
1. Remove `GraphLensKind` type (line 157)
2. **Keep `loadCommandFixtures`** (lines 336-340) — it is used by non-graph fallback functions (e.g. `resolveSourceFallback`, `parseConfigFallback`, etc.)
3. Remove `isGraphTimeoutError()` (lines 365-367)
4. Remove `invokeGraphCommand()` (lines 369-389)
5. Remove `loadFileGraph()` (lines 479-483)
6. Remove `loadFeatureGraph()` (lines 485-489)
7. Remove `loadDataflowGraph()` (lines 491-500)
8. Remove `loadSymbolGraph()` (lines 502-506)

- [ ] **Step 3: Remove graph fallbacks from commands.fixtures.ts**

In `ui/src/ipc/commands.fixtures.ts`:
1. Remove `FALLBACK_FILE_GRAPH` constant (lines 149-177)
2. Remove `FALLBACK_FEATURE_GRAPH` constant (lines 179-192)
3. Remove `FALLBACK_DATAFLOW_GRAPH_REDACTED` and `FALLBACK_DATAFLOW_GRAPH_VALUES` constants (lines 194-226)
4. Remove `FALLBACK_SYMBOL_GRAPH` constant (lines 228-267)
5. Remove `loadFileGraphFallback()` (lines 585-587)
6. Remove `loadFeatureGraphFallback()` (lines 589-591)
7. Remove `loadDataflowGraphFallback()` (lines 593-601)
8. Remove `loadSymbolGraphFallback()` (lines 603-605)
9. Remove any `ProjectGraphResponse` import that becomes unused

- [ ] **Step 4: Verify TypeScript compiles for the modified IPC files**

Run: `cd ui && npx tsc --noEmit 2>&1 | head -30`
Expected: Errors in `useUnifiedGraph.ts` (still imports mockGraph) — that's expected and fixed in Task 7. The IPC files (`transport.ts`, `commands.ts`, `commands.fixtures.ts`) themselves should have no errors.

**Note:** Do NOT delete `mockGraph.ts` yet — `useUnifiedGraph.ts` still imports it. That file gets fully rewritten in Task 7, after which `mockGraph.ts` can be deleted in Task 7 Step 2.

- [ ] **Step 5: Commit**

```bash
git add -u ui/src/ipc/transport.ts ui/src/ipc/commands.ts ui/src/ipc/commands.fixtures.ts
git commit -m "refactor(explorer-api): remove old graph commands, routes, fixtures, and fallbacks"
```

---

## Task 6: Frontend Type Updates and New Transport Route

**Files:**
- Modify: `ui/src/features/workstation/CodebaseExplorer/types.ts`
- Modify: `ui/src/ipc/transport.ts`
- Modify: `ui/src/ipc/commands.ts`

- [ ] **Step 1: Add `childCount` to ExplorerNode in types.ts**

In `ui/src/features/workstation/CodebaseExplorer/types.ts`, at line 27 (inside the `ExplorerNode` type, after `signature?`):

```typescript
  childCount?: number;
```

- [ ] **Step 2: Add new fields to ExplorerContextValue in types.ts**

In the `ExplorerContextValue` type, add these fields:

```typescript
  // Data loading
  nodeMap: Map<string, ExplorerNode>;
  isLoading: boolean;
  loadingClusters: Set<string>;
  error: string | null;
  isStale: boolean;
  expandCluster: (clusterId: string) => void;
  reload: () => void;
```

- [ ] **Step 3: Add the transport route for explorer-graph**

In `ui/src/ipc/transport.ts`, add to `COMMAND_ROUTES`:

```typescript
  load_explorer_graph: {
    method: "GET",
    path: (args) => {
      const sid = encodeURIComponent(String(args.session_id ?? ""));
      const params = new URLSearchParams();
      if (args.depth) params.set("depth", String(args.depth));
      if (args.cluster) params.set("cluster", String(args.cluster));
      const qs = params.toString();
      return `/api/sessions/${sid}/explorer-graph${qs ? `?${qs}` : ""}`;
    },
  },
```

- [ ] **Step 4: Add response types and command function in commands.ts**

In `ui/src/ipc/commands.ts`, add the response types:

```typescript
export type ExplorerGraphResponse = {
  sessionId: string;
  nodes: ExplorerNodeResponse[];
  edges: ExplorerEdgeResponse[];
};

export type ExplorerNodeResponse = {
  id: string;
  label: string;
  kind: string;
  filePath?: string;
  line?: number;
  signature?: {
    parameters: { name: string; typeAnnotation?: string; position: number }[];
    returnType?: string;
  };
  childCount?: number;
};

export type ExplorerEdgeResponse = {
  from: string;
  to: string;
  relation: string;
  parameterName?: string;
  parameterPosition?: number;
  valuePreview?: string;
};
```

Add the command function with tiered timeout:

```typescript
export async function loadExplorerGraph(
  sessionId: string,
  depth?: "overview" | "full",
  cluster?: string,
): Promise<ExplorerGraphResponse> {
  const timeoutMs = cluster ? 5_000 : depth === "full" ? 15_000 : 3_000;
  const result = getTransport().invoke<ExplorerGraphResponse>(
    "load_explorer_graph",
    { session_id: sessionId, depth, cluster },
  );
  const timeout = new Promise<never>((_, reject) =>
    setTimeout(
      () => reject(new Error(`Request timed out after ${timeoutMs / 1000}s`)),
      timeoutMs,
    ),
  );
  return Promise.race([result, timeout]);
}
```

- [ ] **Step 5: Verify it compiles**

Run: `cd ui && npx tsc --noEmit 2>&1 | head -20`
Expected: May show errors in files still importing old fixtures — those are fixed in subsequent tasks.

- [ ] **Step 6: Commit**

```bash
git add ui/src/features/workstation/CodebaseExplorer/types.ts ui/src/ipc/transport.ts ui/src/ipc/commands.ts
git commit -m "feat(explorer-api): add frontend response types, transport route, and loadExplorerGraph command"
```

---

## Task 7: Rewrite useUnifiedGraph Hook

**Files:**
- Modify: `ui/src/features/workstation/CodebaseExplorer/hooks/useUnifiedGraph.ts`

- [ ] **Step 1: Rewrite the hook with API loading and on-demand expansion**

Replace the entire content of `ui/src/features/workstation/CodebaseExplorer/hooks/useUnifiedGraph.ts`:

```typescript
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { loadExplorerGraph } from "../../../../ipc/commands";
import type { ExplorerGraphResponse } from "../../../../ipc/commands";
import type { ExplorerEdge, ExplorerGraph, ExplorerNode } from "../types";
import { getTransport } from "../../../../ipc/transport";

function toExplorerNode(n: ExplorerGraphResponse["nodes"][number]): ExplorerNode {
  return {
    id: n.id,
    label: n.label,
    kind: n.kind as ExplorerNode["kind"],
    filePath: n.filePath,
    line: n.line,
    signature: n.signature
      ? {
          parameters: n.signature.parameters.map((p) => ({
            name: p.name,
            typeAnnotation: p.typeAnnotation,
            position: p.position,
          })),
          returnType: n.signature.returnType,
        }
      : undefined,
    childCount: n.childCount,
  };
}

function toExplorerEdge(e: ExplorerGraphResponse["edges"][number]): ExplorerEdge {
  return {
    from: e.from,
    to: e.to,
    relation: e.relation as ExplorerEdge["relation"],
    parameterName: e.parameterName,
    parameterPosition: e.parameterPosition,
    valuePreview: e.valuePreview,
  };
}

function mergeClusterData(
  current: ExplorerGraph,
  expansion: ExplorerGraphResponse,
): ExplorerGraph {
  const existingIds = new Set(current.nodes.map((n) => n.id));
  const newNodes = expansion.nodes
    .filter((n) => !existingIds.has(n.id))
    .map(toExplorerNode);
  const existingEdgeKeys = new Set(
    current.edges.map((e) => `${e.from}\u2192${e.to}\u2192${e.relation}`),
  );
  const newEdges = expansion.edges
    .filter((e) => !existingEdgeKeys.has(`${e.from}\u2192${e.to}\u2192${e.relation}`))
    .map(toExplorerEdge);
  return {
    nodes: [...current.nodes, ...newNodes],
    edges: [...current.edges, ...newEdges],
  };
}

const EMPTY_GRAPH: ExplorerGraph = { nodes: [], edges: [] };

export function useUnifiedGraph(sessionId: string): {
  graph: ExplorerGraph;
  nodeMap: Map<string, ExplorerNode>;
  isLoading: boolean;
  loadingClusters: Set<string>;
  error: string | null;
  isStale: boolean;
  expandCluster: (clusterId: string) => void;
  reload: () => void;
} {
  const [graph, setGraph] = useState<ExplorerGraph>(EMPTY_GRAPH);
  const [isLoading, setIsLoading] = useState(true);
  const [loadingClusters, setLoadingClusters] = useState<Set<string>>(new Set());
  const [loadedClusters, setLoadedClusters] = useState<Set<string>>(new Set());
  const [error, setError] = useState<string | null>(null);
  const [isStale, setIsStale] = useState(false);
  const generationRef = useRef(0);

  // O(1) node index
  const nodeMap = useMemo(() => {
    const map = new Map<string, ExplorerNode>();
    for (const node of graph.nodes) {
      map.set(node.id, node);
    }
    return map;
  }, [graph.nodes]);

  // Initial load
  useEffect(() => {
    const gen = ++generationRef.current;
    setIsLoading(true);
    setError(null);
    setGraph(EMPTY_GRAPH);
    setLoadingClusters(new Set());
    setLoadedClusters(new Set());
    setIsStale(false);

    void loadExplorerGraph(sessionId, "overview").then(
      (resp) => {
        if (gen !== generationRef.current) return;
        setGraph({
          nodes: resp.nodes.map(toExplorerNode),
          edges: resp.edges.map(toExplorerEdge),
        });
        setIsLoading(false);
      },
      (err) => {
        if (gen !== generationRef.current) return;
        setError(err instanceof Error ? err.message : "Failed to load graph");
        setIsLoading(false);
      },
    );
  }, [sessionId]);

  // Stale data subscription
  useEffect(() => {
    const unsubscribe = getTransport().subscribe<{ event: string }>(
      "explorer_graph_stale",
      sessionId,
      (payload) => {
        if (payload.event === "explorer_graph_stale" && !isLoading) {
          setIsStale(true);
        }
      },
    );
    return unsubscribe;
  }, [sessionId, isLoading]);

  const expandCluster = useCallback(
    (clusterId: string) => {
      // Skip if already loaded
      if (loadedClusters.has(clusterId)) return;

      const gen = generationRef.current;
      setLoadingClusters((prev) => new Set(prev).add(clusterId));

      void loadExplorerGraph(sessionId, undefined, clusterId).then(
        (resp) => {
          if (gen !== generationRef.current) return;
          setGraph((prev) => mergeClusterData(prev, resp));
          setLoadedClusters((prev) => new Set(prev).add(clusterId));
          setLoadingClusters((prev) => {
            const next = new Set(prev);
            next.delete(clusterId);
            return next;
          });
        },
        () => {
          if (gen !== generationRef.current) return;
          setLoadingClusters((prev) => {
            const next = new Set(prev);
            next.delete(clusterId);
            return next;
          });
        },
      );
    },
    [sessionId, loadedClusters],
  );

  const reload = useCallback(() => {
    generationRef.current++;
    setIsStale(false);
    setIsLoading(true);
    setError(null);
    setGraph(EMPTY_GRAPH);
    setLoadingClusters(new Set());
    setLoadedClusters(new Set());

    const gen = generationRef.current;
    void loadExplorerGraph(sessionId, "overview").then(
      (resp) => {
        if (gen !== generationRef.current) return;
        setGraph({
          nodes: resp.nodes.map(toExplorerNode),
          edges: resp.edges.map(toExplorerEdge),
        });
        setIsLoading(false);
      },
      (err) => {
        if (gen !== generationRef.current) return;
        setError(err instanceof Error ? err.message : "Failed to load graph");
        setIsLoading(false);
      },
    );
  }, [sessionId]);

  return { graph, nodeMap, isLoading, loadingClusters, error, isStale, expandCluster, reload };
}
```

- [ ] **Step 2: Delete the old fixture data file (now safe — no imports remain)**

```bash
rm ui/src/features/workstation/CodebaseExplorer/fixtures/mockGraph.ts
```

- [ ] **Step 3: Verify it compiles**

Run: `cd ui && npx tsc --noEmit 2>&1 | grep useUnifiedGraph`
Expected: No errors in this file (other files may error because they reference old imports)

- [ ] **Step 4: Commit**

```bash
git add ui/src/features/workstation/CodebaseExplorer/hooks/useUnifiedGraph.ts
git rm ui/src/features/workstation/CodebaseExplorer/fixtures/mockGraph.ts
git commit -m "feat(explorer-api): rewrite useUnifiedGraph with API loading and on-demand cluster expansion"
```

---

## Task 8: Rewrite AdaptiveLayout — Replace ID Parsing with ParentMap

**Files:**
- Modify: `ui/src/features/workstation/CodebaseExplorer/AdaptiveLayout.tsx` (lines 21-50, 76-78)

- [ ] **Step 1: Replace `parentModuleId()` and `parentCrateId()` with `buildParentMap()`**

Replace the `parentModuleId` function (lines 21-30) and `parentCrateId` function (lines 32-50) with:

```typescript
/**
 * Build a parent lookup from "contains" edges.
 * Returns a Map where key = child node ID, value = parent node ID.
 */
function buildParentMap(edges: ExplorerEdge[]): Map<string, string> {
  const map = new Map<string, string>();
  for (const edge of edges) {
    if (edge.relation === "contains") {
      map.set(edge.to, edge.from);
    }
  }
  return map;
}
```

- [ ] **Step 2: Update `countChildren()` to prefer `node.childCount`**

Replace the `countChildren` function (lines 76-78) with:

```typescript
function countChildren(node: ExplorerNode, edges: ExplorerEdge[]): number {
  if (node.childCount != null) return node.childCount;
  return edges.filter((e) => e.relation === "contains" && e.from === node.id).length;
}
```

Also update the call site at line 143 in `buildReactFlowModel`:
- Change `countChildren(node.id, graph)` → `countChildren(node, graph.edges)`

- [ ] **Step 3: Update all call sites of `parentModuleId` / `parentCrateId`**

Search for all calls to `parentModuleId(...)` and `parentCrateId(...)` in `AdaptiveLayout.tsx`. Replace with `parentMap.get(nodeId)`. The `parentMap` should be built once at the top of the `buildReactFlowModel` function:

```typescript
const parentMap = buildParentMap(graph.edges);
```

Then replace patterns like:
- `parentModuleId(node)` → `parentMap.get(node.id)`
- `parentCrateId(node)` → look up parent chain: `parentMap.get(parentMap.get(node.id) ?? "")`

- [ ] **Step 4: Verify it compiles**

Run: `cd ui && npx tsc --noEmit 2>&1 | grep AdaptiveLayout`
Expected: No errors

- [ ] **Step 5: Commit**

```bash
git add ui/src/features/workstation/CodebaseExplorer/AdaptiveLayout.tsx
git commit -m "refactor(explorer-api): replace ID-parsing hierarchy functions with contains-edge parentMap"
```

---

## Task 9: Wire ExplorerContext and index.tsx

**Files:**
- Modify: `ui/src/features/workstation/CodebaseExplorer/ExplorerContext.tsx`
- Modify: `ui/src/features/workstation/CodebaseExplorer/index.tsx`

- [ ] **Step 1: Update ExplorerProvider to accept sessionId and wire new hook fields**

In `ExplorerContext.tsx`:

1. Add `sessionId: string` to the props type (currently it receives `onNavigateToSource` — add `sessionId` alongside it).
2. Change `useUnifiedGraph()` call (line 33) to `useUnifiedGraph(sessionId)` and destructure all new fields:
   ```typescript
   const { graph, nodeMap, isLoading, loadingClusters, error, isStale, expandCluster, reload } =
     useUnifiedGraph(sessionId);
   ```
3. Add all new fields to the context value object (around lines 100-155):
   ```typescript
   nodeMap,
   isLoading,
   loadingClusters,
   error,
   isStale,
   expandCluster,
   reload,
   ```

- [ ] **Step 2: Update index.tsx to pass sessionId to ExplorerProvider**

In `index.tsx`:
1. Change `sessionId: _sessionId,` (line 91) to `sessionId,` (remove the underscore rename since it will now be used).
2. Pass `sessionId` as a prop to `ExplorerProvider`:
   ```typescript
   <ExplorerProvider sessionId={sessionId} onNavigateToSource={onNavigateToSource}>
   ```

- [ ] **Step 3: Verify it compiles**

Run: `cd ui && npx tsc --noEmit 2>&1 | grep -E "ExplorerContext|index.tsx" | head -10`
Expected: No errors in these files

- [ ] **Step 4: Commit**

```bash
git add ui/src/features/workstation/CodebaseExplorer/ExplorerContext.tsx ui/src/features/workstation/CodebaseExplorer/index.tsx
git commit -m "feat(explorer-api): wire sessionId through ExplorerProvider and expose new context fields"
```

---

## Task 10: Loading, Error, and Stale UI States

**Files:**
- Modify: `ui/src/features/workstation/CodebaseExplorer/index.tsx`
- Modify: `ui/src/styles.css`

- [ ] **Step 1: Add loading/error/stale UI to index.tsx**

In `index.tsx`, inside the `ExplorerLayout` component (which renders the toolbar + canvas + context panel), add conditional rendering based on `isLoading`, `error`, `isStale` from context:

```typescript
// Inside ExplorerLayout, before the canvas:
const { isLoading, error, isStale, reload } = useExplorer();

// Stale banner (above toolbar)
{isStale && (
  <div className="explorer-stale-banner" role="status">
    <span>Graph data has been updated.</span>
    <button type="button" onClick={reload}>Reload</button>
  </div>
)}

// Replace canvas area when loading or error:
{isLoading ? (
  <div className="explorer-loading" role="status" aria-label="Loading graph">
    <div className="explorer-spinner" />
    <p>Loading project graph...</p>
  </div>
) : error ? (
  <div className="explorer-error" role="alert">
    <p>{error}</p>
    <button type="button" onClick={reload}>Retry</button>
  </div>
) : (
  // existing canvas + context panel
)}
```

Also disable toolbar controls when `isLoading || !!error` by adding a `disabled` prop or `aria-disabled`.

- [ ] **Step 2: Add CSS for loading, error, and stale states**

In `ui/src/styles.css`, add after the existing explorer CSS:

```css
.explorer-stale-banner {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 12px;
  background: #3a3d41;
  border-bottom: 1px solid #252526;
  font-size: 12px;
  color: #cccccc;
}

.explorer-stale-banner button {
  border: 1px solid #0e639c;
  border-radius: 3px;
  background: #094771;
  color: #ffffff;
  padding: 2px 8px;
  font-size: 11px;
  cursor: pointer;
}

.explorer-loading,
.explorer-error {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
  flex: 1;
  color: #8b8b8b;
  font-size: 13px;
}

.explorer-error {
  color: #f48771;
}

.explorer-error button {
  border: 1px solid #f48771;
  border-radius: 3px;
  background: transparent;
  color: #f48771;
  padding: 4px 12px;
  font-size: 12px;
  cursor: pointer;
}

.explorer-spinner {
  width: 24px;
  height: 24px;
  border: 2px solid #3c3c3c;
  border-top-color: #0e639c;
  border-radius: 50%;
  animation: explorer-spin 0.8s linear infinite;
}

@keyframes explorer-spin {
  to { transform: rotate(360deg); }
}
```

- [ ] **Step 3: Verify it renders**

Run: `cd ui && npx vitest run src/features/workstation/CodebaseExplorer 2>&1 | tail -10`
Expected: Existing tests may fail due to mocking changes — that's expected and fixed in Task 13.

- [ ] **Step 4: Commit**

```bash
git add ui/src/features/workstation/CodebaseExplorer/index.tsx ui/src/styles.css
git commit -m "feat(explorer-api): add loading, error, and stale data UI states"
```

---

## Task 11: ClusterNode Loading Indicator and ExplorerCanvas Expansion

**Files:**
- Modify: `ui/src/features/workstation/CodebaseExplorer/nodes/ClusterNode.tsx`
- Modify: `ui/src/features/workstation/CodebaseExplorer/ExplorerCanvas.tsx`

- [ ] **Step 1: Update ClusterNode to show spinner when loading**

In `ClusterNode.tsx`:

1. Change the component signature to accept `NodeProps<ClusterNodeData>` from ReactFlow (which provides `id` alongside `data`):
   ```typescript
   import type { NodeProps } from "reactflow";
   import { useExplorer } from "../ExplorerContext";
   ```

2. Change the component signature from `({ data }: { data: ClusterNodeData })` to `({ id, data }: NodeProps<ClusterNodeData>)`.

3. Access `loadingClusters` from context and conditionally render a spinner:

```typescript
const { loadingClusters } = useExplorer();
const isExpanding = loadingClusters.has(id);

// Replace the expand/collapse toggle span with:
{isExpanding ? (
  <span className="explorer-spinner" style={{ width: 14, height: 14 }} aria-label="Loading cluster" />
) : (
  <span className="explorer-cluster-toggle" aria-label={data.expanded ? "collapse" : "expand"}>
    {data.expanded ? "▾" : "▸"}
  </span>
)}
```

- [ ] **Step 2: Update ExplorerCanvas to call expandCluster on cluster click**

In `ExplorerCanvas.tsx`, the `handleNodeClick` callback at line 133 currently calls `ctx.toggleCluster(node.id)` for cluster nodes. Update it to also call `expandCluster`:

```typescript
if (node.type === "clusterNode") {
  ctx.expandCluster(node.id);  // triggers API fetch if not already loaded
  ctx.toggleCluster(node.id);   // toggles UI expand/collapse
  return;
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cd ui && npx tsc --noEmit 2>&1 | grep -E "ClusterNode|ExplorerCanvas" | head -5`
Expected: No errors

- [ ] **Step 4: Commit**

```bash
git add ui/src/features/workstation/CodebaseExplorer/nodes/ClusterNode.tsx ui/src/features/workstation/CodebaseExplorer/ExplorerCanvas.tsx
git commit -m "feat(explorer-api): add cluster loading indicator and on-demand expansion trigger"
```

---

## Task 12: Rewrite Hook Unit Tests

**Files:**
- Modify: `ui/src/features/workstation/CodebaseExplorer/__tests__/hooks.test.ts`

- [ ] **Step 1: Remove fixture imports and fixture validation tests**

Remove:
1. The import of `smallFixture`, `mediumFixture`, `largeFixture` from `../fixtures/mockGraph` (line 4)
2. The entire `describe("fixture data", ...)` block (lines 16-55)

- [ ] **Step 2: Add test helper and mock for loadExplorerGraph**

Add at the top of the file:

```typescript
import { act, renderHook, waitFor } from "@testing-library/react";
import { vi } from "vitest";
import type { ExplorerGraph, ExplorerNode, ExplorerEdge } from "../types";
import { useUnifiedGraph } from "../hooks/useUnifiedGraph";

// Mock the commands module
vi.mock("../../../../ipc/commands", () => ({
  loadExplorerGraph: vi.fn(),
}));

vi.mock("../../../../ipc/transport", () => ({
  getTransport: () => ({
    subscribe: vi.fn(() => vi.fn()),
  }),
}));

import { loadExplorerGraph } from "../../../../ipc/commands";

function makeTestGraph(overrides?: Partial<ExplorerGraph>): ExplorerGraph {
  return {
    nodes: (overrides?.nodes as ExplorerNode[]) ?? [
      { id: "sym_aaa", label: "foo", kind: "function" },
      { id: "sym_bbb", label: "bar", kind: "function" },
    ],
    edges: (overrides?.edges as ExplorerEdge[]) ?? [
      { from: "sym_aaa", to: "sym_bbb", relation: "calls" },
    ],
  };
}
```

- [ ] **Step 3: Add useUnifiedGraph tests**

```typescript
describe("useUnifiedGraph", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("calls loadExplorerGraph with overview on mount", async () => {
    const mockResponse = {
      sessionId: "s1",
      nodes: [{ id: "crt_aaa", label: "mycrate", kind: "crate", childCount: 3 }],
      edges: [],
    };
    (loadExplorerGraph as ReturnType<typeof vi.fn>).mockResolvedValue(mockResponse);

    const { result } = renderHook(() => useUnifiedGraph("s1"));

    expect(result.current.isLoading).toBe(true);
    await waitFor(() => expect(result.current.isLoading).toBe(false));

    expect(loadExplorerGraph).toHaveBeenCalledWith("s1", "overview");
    expect(result.current.graph.nodes).toHaveLength(1);
    expect(result.current.nodeMap.get("crt_aaa")).toBeDefined();
    expect(result.current.error).toBeNull();
  });

  it("sets error on API failure", async () => {
    (loadExplorerGraph as ReturnType<typeof vi.fn>).mockRejectedValue(
      new Error("Network error"),
    );

    const { result } = renderHook(() => useUnifiedGraph("s1"));
    await waitFor(() => expect(result.current.isLoading).toBe(false));

    expect(result.current.error).toBe("Network error");
    expect(result.current.graph.nodes).toHaveLength(0);
  });

  it("expandCluster merges without duplicates", async () => {
    const overviewResp = {
      sessionId: "s1",
      nodes: [{ id: "crt_aaa", label: "mycrate", kind: "crate", childCount: 1 }],
      edges: [],
    };
    const clusterResp = {
      sessionId: "s1",
      nodes: [
        { id: "crt_aaa", label: "mycrate", kind: "crate" }, // duplicate
        { id: "fil_bbb", label: "lib.rs", kind: "file" },
      ],
      edges: [{ from: "crt_aaa", to: "fil_bbb", relation: "contains" }],
    };
    (loadExplorerGraph as ReturnType<typeof vi.fn>)
      .mockResolvedValueOnce(overviewResp)
      .mockResolvedValueOnce(clusterResp);

    const { result } = renderHook(() => useUnifiedGraph("s1"));
    await waitFor(() => expect(result.current.isLoading).toBe(false));

    act(() => result.current.expandCluster("crt_aaa"));
    await waitFor(() => expect(result.current.graph.nodes).toHaveLength(2));

    // No duplicate crt_aaa
    const crateNodes = result.current.graph.nodes.filter((n) => n.id === "crt_aaa");
    expect(crateNodes).toHaveLength(1);
  });
});
```

- [ ] **Step 4: Update existing hook tests to use inline data instead of fixtures**

For `useFocusContext` and `useTrace` tests, replace any reference to fixture data with `makeTestGraph()` calls. These hooks take a graph as input, so pass inline graphs directly.

- [ ] **Step 5: Run tests**

Run: `cd ui && npx vitest run src/features/workstation/CodebaseExplorer/__tests__/hooks.test.ts`
Expected: All tests pass

- [ ] **Step 6: Commit**

```bash
git add ui/src/features/workstation/CodebaseExplorer/__tests__/hooks.test.ts
git commit -m "test(explorer-api): rewrite hook tests with mocked API and inline test data"
```

---

## Task 13: Rewrite Integration Tests

**Files:**
- Modify: `ui/src/features/workstation/CodebaseExplorer/__tests__/CodebaseExplorer.test.tsx`

- [ ] **Step 1: Add mock for loadExplorerGraph**

At the top of the file, add a mock for the commands module:

```typescript
const mockLoadExplorerGraph = vi.fn();
vi.mock("../../../../ipc/commands", () => ({
  loadExplorerGraph: (...args: unknown[]) => mockLoadExplorerGraph(...args),
}));

vi.mock("../../../../ipc/transport", () => ({
  getTransport: () => ({
    subscribe: vi.fn(() => vi.fn()),
  }),
}));
```

- [ ] **Step 2: Set up default mock response in beforeEach**

```typescript
const MOCK_OVERVIEW_RESPONSE = {
  sessionId: "test-session",
  nodes: [
    { id: "crt_001", label: "engine-crypto", kind: "crate", childCount: 3 },
    { id: "mod_002", label: "src", kind: "module", childCount: 2 },
    { id: "fil_003", label: "verify.rs", kind: "file", filePath: "engine-crypto/src/verify.rs" },
  ],
  edges: [
    { from: "crt_001", to: "mod_002", relation: "contains" },
    { from: "mod_002", to: "fil_003", relation: "contains" },
  ],
};

beforeEach(() => {
  vi.clearAllMocks();
  mockLoadExplorerGraph.mockResolvedValue(MOCK_OVERVIEW_RESPONSE);
});
```

- [ ] **Step 3: Add loading state test**

```typescript
it("shows loading state initially", () => {
  // Make the API call hang (never resolve)
  mockLoadExplorerGraph.mockReturnValue(new Promise(() => {}));

  render(<CodebaseExplorer sessionId="test-session" />);
  expect(screen.getByText("Loading project graph...")).toBeInTheDocument();
});
```

- [ ] **Step 4: Add error state test**

```typescript
it("shows error state on API failure", async () => {
  mockLoadExplorerGraph.mockRejectedValue(new Error("Connection refused"));

  render(<CodebaseExplorer sessionId="test-session" />);
  await waitFor(() => {
    expect(screen.getByText("Connection refused")).toBeInTheDocument();
  });
  expect(screen.getByText("Retry")).toBeInTheDocument();
});
```

- [ ] **Step 5: Add overview render test**

```typescript
it("renders cluster nodes after overview loads", async () => {
  render(<CodebaseExplorer sessionId="test-session" />);
  await waitFor(() => {
    expect(screen.getByText("engine-crypto")).toBeInTheDocument();
  });
});
```

- [ ] **Step 6: Update existing FOCUS/Esc tests**

Update any tests that relied on fixture-provided node names (like `verify_signature`, `hash_blake3`) to use node names from `MOCK_OVERVIEW_RESPONSE` instead.

- [ ] **Step 7: Run integration tests**

Run: `cd ui && npx vitest run src/features/workstation/CodebaseExplorer/__tests__/CodebaseExplorer.test.tsx`
Expected: All tests pass

- [ ] **Step 8: Run all workstation tests to verify nothing is broken**

Run: `cd ui && npx vitest run src/features/workstation/ 2>&1 | tail -10`
Expected: All test files pass

- [ ] **Step 9: Commit**

```bash
git add ui/src/features/workstation/CodebaseExplorer/__tests__/CodebaseExplorer.test.tsx
git commit -m "test(explorer-api): rewrite integration tests with mocked API, add loading/error state coverage"
```

---

## Verification Checklist

After all 13 tasks are complete, run these verification steps:

- [ ] **Backend compiles:** `cargo check -p session-manager -p web-server`
- [ ] **Backend tests pass:** `cargo test -p session-manager -- explorer_graph`
- [ ] **Frontend compiles:** `cd ui && npx tsc --noEmit`
- [ ] **Frontend tests pass:** `cd ui && npx vitest run src/features/workstation/`
- [ ] **No references to old fixtures remain:** `grep -r "mockGraph\|smallFixture\|mediumFixture\|largeFixture" ui/src/ --include="*.ts" --include="*.tsx"` returns no results
- [ ] **No references to old graph commands remain:** `grep -r "loadFileGraph\|loadFeatureGraph\|loadDataflowGraph\|loadSymbolGraph\|GRAPH_TIMEOUT_MS\|GRAPH_COMMANDS\|invokeGraphCommand\|isGraphTimeoutError\|GraphLensKind" ui/src/ --include="*.ts" --include="*.tsx"` returns no results
- [ ] **Old fixture file deleted:** `ls ui/src/features/workstation/CodebaseExplorer/fixtures/` returns empty or directory not found
