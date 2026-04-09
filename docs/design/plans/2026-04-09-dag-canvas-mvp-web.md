# DAG Canvas MVP Web-UI Refinement Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make the web-UI DAG canvas a solid, truthful, interactive code exploration tool — graph + code visible simultaneously, no fake affordances, honest about what it can and cannot trace.

**Architecture:** Fix the web layout (only — desktop Allotment path untouched) to a persistent split view (file tree | code + graph | context panel). Strip fake parameter-trace UI, replace with honest caller/callee neighborhood highlighting (set of reachable nodes, not single-path trace). Wire real source snippets into the context panel with stale-response guards. Fix backend hierarchy builder for nested modules. Make small-repo default view explorable using existing file-count thresholds with an explicit two-stage `overview -> full` upgrade. Scope search honestly to the currently loaded graph, backed by explicit graph-depth state rather than heuristic inference.

**Tech Stack:** React 18, ReactFlow, ELK.js, Axum (backend), Rust project-ir

**Guiding Principle:** The teammate review nailed it — "not more graph features; making the existing graph truthful, explorable, and persistent." Every task below either removes a lie or makes something real work.

---

## Phase 1: Backend Truthfulness

### Task 1: Fix nested module hierarchy in explorer_graph.rs

The current builder flattens all directories to a single module node hung directly off the crate. For `crypto/src/zk/circom/`, only `circom` gets a module node, attached to `crypto` — losing the `zk` intermediate.

**Files:**
- Modify: `crates/services/session-manager/src/explorer_graph.rs:171-219` — `build_hierarchy` method
- Modify: `crates/services/session-manager/src/explorer_graph_tests.rs` — add nested module test

**Step 1: Write test for nested module hierarchy**

Add to `explorer_graph_tests.rs`:

```rust
#[test]
fn nested_modules_produce_parent_chain() {
    let root = PathBuf::from("/project");
    let ir = make_nested_ir(&root);
    let builder = ExplorerGraphBuilder::new(&ir, &root);

    let response = builder
        .build("sess-1", ExplorerDepth::Overview, None)
        .expect("overview should build");

    // Should have: crate(mycrate), module(src), module(zk), module(circom), file(lib.rs)
    let modules: Vec<_> = response.nodes.iter().filter(|n| n.kind == "module").collect();
    assert!(modules.len() >= 2, "expected at least 2 module levels, got {}", modules.len());

    // There should be a contains edge from zk-module to circom-module
    let module_ids: Vec<_> = modules.iter().map(|n| &n.id).collect();
    let has_module_to_module = response.edges.iter().any(|e| {
        e.relation == "contains"
            && module_ids.contains(&&e.from)
            && module_ids.contains(&&e.to)
    });
    assert!(has_module_to_module, "expected contains edge between parent and child modules");
}

fn make_nested_ir(root: &Path) -> ProjectIr {
    let file_nodes = vec![
        FileNode {
            id: "file:mycrate/src/zk/circom/lib.rs".to_string(),
            path: root.join("mycrate/src/zk/circom/lib.rs"),
            language: "rust".to_string(),
        },
        FileNode {
            id: "file:mycrate/src/lib.rs".to_string(),
            path: root.join("mycrate/src/lib.rs"),
            language: "rust".to_string(),
        },
    ];

    ProjectIr {
        file_graph: Graph { nodes: file_nodes, edges: vec![] },
        symbol_graph: Graph::default(),
        feature_graph: Graph::default(),
        dataflow_graph: Graph::default(),
        framework_views: vec![],
    }
}
```

**Step 2: Run test, verify it fails**

Run: `cargo test -p session-manager nested_modules -- --nocapture`
Expected: FAIL — only one module node produced

**Step 3: Fix `build_hierarchy` to chain intermediate modules**

In `explorer_graph.rs`, replace the module-creation block in `build_hierarchy` (lines 171-219). The fix: iterate segment pairs and create module nodes for every intermediate directory, chaining contains edges from parent to child module.

```rust
// Replace the if segments.len() > 2 block with:
let mut parent_id = crate_temp_id.clone();

// Build module chain for intermediate directories (skip crate prefix and filename)
// segments = ["mycrate", "src", "zk", "circom", "lib.rs"]
// module dirs = ["mycrate/src", "mycrate/src/zk", "mycrate/src/zk/circom"]
for depth in 1..segments.len().saturating_sub(1) {
    let dir_path = segments[..=depth].join("/");
    let module_temp_id = format!("__module:{dir_path}");

    if !seen_modules.contains_key(&dir_path) {
        seen_modules.insert(dir_path.clone(), module_temp_id.clone());
        let module_label = segments[depth].to_string();
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
            from: parent_id.clone(),
            to: module_temp_id.clone(),
            relation: "contains".to_string(),
            parameter_name: None,
            parameter_position: None,
            value_preview: None,
        });
    }

    parent_id = seen_modules
        .get(&dir_path)
        .cloned()
        .unwrap_or(parent_id);
}

// Attach file to its direct parent (deepest module or crate)
edges.push(ExplorerEdgeResponse {
    from: parent_id,
    to: file_temp_id,
    relation: "contains".to_string(),
    parameter_name: None,
    parameter_position: None,
    value_preview: None,
});
```

This replaces the entire `if segments.len() > 2 { ... } else { ... }` block.

**Step 4: Run tests, verify pass**

Run: `cargo test -p session-manager -- --nocapture`
Expected: all tests pass including `nested_modules_produce_parent_chain`

**Step 5: Commit**

```bash
git add crates/services/session-manager/src/explorer_graph.rs crates/services/session-manager/src/explorer_graph_tests.rs
git commit -m "fix(explorer-graph): chain intermediate module nodes for nested directories"
```

---

### Task 2: Propagate dataflow parameter_name from IR edges

The backend `add_cross_file_edges` hardcodes `parameter_name: None` on every emitted edge, even though the `ExplorerEdgeResponse` schema supports it. The dataflow IR edges carry `value_preview` but neither `BasicEdge` nor `DataflowEdge` carries `parameter_name`. Since the IR doesn't have this data, the honest fix is: **don't pretend we have it.** But we should still propagate `value_preview` correctly (it's already half-done) and use the edge `relation` field truthfully.

**Files:**
- Modify: `crates/services/session-manager/src/explorer_graph.rs:288-316` — `add_cross_file_edges`
- Modify: `crates/services/session-manager/src/explorer_graph_tests.rs` — add edge propagation test

**Step 1: Write test for value_preview propagation**

```rust
#[test]
fn cross_file_edges_propagate_value_preview() {
    let root = PathBuf::from("/project");
    let ir = make_test_ir(&root);
    let builder = ExplorerGraphBuilder::new(&ir, &root);

    let response = builder
        .build("sess-1", ExplorerDepth::Full, None)
        .expect("full graph should build");

    // The dataflow self-edge has value_preview "true" in make_test_ir
    // It's a self-edge so it gets filtered by dedupe (from==to after hash),
    // but verify that non-self dataflow edges would carry value_preview.
    // For now, just verify no edges claim parameter_name when IR doesn't have it.
    for edge in &response.edges {
        if edge.relation == "calls" || edge.relation == "contains" {
            assert!(
                edge.parameter_name.is_none(),
                "calls/contains edges should not claim parameter_name"
            );
        }
    }
}
```

**Step 2: Run test, verify it passes (this is a correctness assertion, not TDD red-green)**

Run: `cargo test -p session-manager cross_file_edges -- --nocapture`

**Step 3: Commit**

```bash
git add crates/services/session-manager/src/explorer_graph_tests.rs
git commit -m "test(explorer-graph): assert edges don't fabricate parameter_name"
```

---

## Phase 2: Remove Fake Affordances from UI

### Task 3: Strip parameter-trace UI, replace with honest neighborhood highlighting

The SymbolNode exposes per-parameter trace buttons and the ContextPanel exposes "Trace origin of X" — but the backend never populates `parameter_name` on edges. The trace hook falls back to generic BFS over `calls` edges and returns the deepest endpoint as a single "path," which is traversal-order-dependent and arbitrary on branched call graphs. Remove the per-parameter illusion AND the single-path reconstruction. Replace with honest neighborhood highlighting: BFS collects all reachable caller/callee nodes and highlights them. Label the actions "Show callers" / "Show callees" (not "Trace") to avoid implying a definitive path exists.

**Files:**
- Modify: `ui/src/features/workstation/CodebaseExplorer/nodes/SymbolNode.tsx`
- Modify: `ui/src/features/workstation/CodebaseExplorer/ContextPanel.tsx`
- Modify: `ui/src/features/workstation/CodebaseExplorer/ExplorerContext.tsx`
- Modify: `ui/src/features/workstation/CodebaseExplorer/types.ts`
- Modify: `ui/src/features/workstation/CodebaseExplorer/hooks/useTrace.ts`
- Modify: `ui/src/features/workstation/CodebaseExplorer/ExplorerCanvas.tsx`
- Modify: `ui/src/features/workstation/CodebaseExplorer/TraceOverlay.tsx`
- Modify: `ui/src/features/workstation/CodebaseExplorer/AdaptiveLayout.tsx`

**Step 1: Simplify SymbolNode — remove per-parameter click handlers**

Replace `SymbolNode.tsx` with a version that displays the signature read-only (no click handlers on individual parameters):

```tsx
import { Handle, Position } from "reactflow";
import type { FunctionSignature } from "../types";

type SymbolNodeData = {
  label: string;
  kind: string;
  signature?: FunctionSignature;
};

export function SymbolNode({ data }: { data: SymbolNodeData }) {
  return (
    <div className="explorer-symbol-node">
      <Handle type="target" position={Position.Top} style={{ visibility: "hidden" }} />
      <div className="explorer-symbol-name">{data.label}</div>
      {data.signature ? (
        <div className="explorer-symbol-sig">
          <span className="explorer-sig-paren">(</span>
          {data.signature.parameters.map((param, index) => (
            <span key={`${param.name}:${param.position}`}>
              {index > 0 ? <span className="explorer-sig-comma">, </span> : null}
              <span className="explorer-sig-param">
                <span className="explorer-sig-param-name">{param.name}</span>
                {param.typeAnnotation ? (
                  <span className="explorer-sig-param-type">: {param.typeAnnotation}</span>
                ) : null}
              </span>
            </span>
          ))}
          <span className="explorer-sig-paren">)</span>
          {data.signature.returnType ? (
            <span className="explorer-sig-return">
              {" -> "}
              <span className="explorer-sig-return-type">{data.signature.returnType}</span>
            </span>
          ) : null}
        </div>
      ) : null}
      <Handle type="source" position={Position.Bottom} style={{ visibility: "hidden" }} />
    </div>
  );
}
```

**Step 2: Replace useTrace with neighborhood highlighting (not single-path)**

Replace the entire hook. Instead of reconstructing one arbitrary BFS path (which is traversal-order-dependent on branched graphs), collect the full set of reachable nodes. The result is a `Set<string>` of highlighted node IDs, not a path:

```ts
import { useCallback, useState } from "react";
import type { ExplorerEdge, ExplorerGraph, NeighborhoodResult } from "../types";

function buildAdjacency(
  edges: ExplorerEdge[],
  direction: "upstream" | "downstream"
): Map<string, string[]> {
  const adj = new Map<string, string[]>();
  for (const edge of edges) {
    if (edge.relation !== "calls") continue;
    const key = direction === "upstream" ? edge.to : edge.from;
    const val = direction === "upstream" ? edge.from : edge.to;
    if (!adj.has(key)) adj.set(key, []);
    adj.get(key)!.push(val);
  }
  return adj;
}

function bfsNeighborhood(
  startId: string,
  adj: Map<string, string[]>
): Set<string> {
  const visited = new Set<string>([startId]);
  let frontier = [startId];

  while (frontier.length > 0) {
    const next: string[] = [];
    for (const current of frontier) {
      for (const neighbor of adj.get(current) ?? []) {
        if (visited.has(neighbor)) continue;
        visited.add(neighbor);
        next.push(neighbor);
      }
    }
    frontier = next;
  }

  return visited;
}

export function useTrace(graph: ExplorerGraph, focusedNodeId: string | null) {
  const [neighborhoodResult, setNeighborhoodResult] = useState<NeighborhoodResult | null>(null);

  const showCallers = useCallback(() => {
    if (!focusedNodeId) return;
    const adj = buildAdjacency(graph.edges, "upstream");
    const reachable = bfsNeighborhood(focusedNodeId, adj);
    if (reachable.size <= 1) { setNeighborhoodResult(null); return; }
    setNeighborhoodResult({ highlightedIds: reachable, direction: "upstream" });
  }, [focusedNodeId, graph.edges]);

  const showCallees = useCallback(() => {
    if (!focusedNodeId) return;
    const adj = buildAdjacency(graph.edges, "downstream");
    const reachable = bfsNeighborhood(focusedNodeId, adj);
    if (reachable.size <= 1) { setNeighborhoodResult(null); return; }
    setNeighborhoodResult({ highlightedIds: reachable, direction: "downstream" });
  }, [focusedNodeId, graph.edges]);

  const clearHighlight = useCallback(() => { setNeighborhoodResult(null); }, []);

  return { neighborhoodResult, showCallers, showCallees, clearHighlight };
}
```

**Step 3: Update types.ts — replace TraceResult with NeighborhoodResult**

Remove `TraceResult` and `parameterName`. Add:

```ts
export type NeighborhoodResult = {
  highlightedIds: Set<string>;
  direction: "upstream" | "downstream";
};
```

This is a set of reachable node IDs, not a single path. No ordering implied.

**Step 4: Update ExplorerContext.tsx**

- Remove `traceParameter`, `traceReturn`, `traceResult` from the context value
- Remove `hasPotentialParameterFlow` and `hasPotentialReturnFlow` helpers
- Replace with `showCallers`, `showCallees`, `neighborhoodResult`, `clearHighlight`
- Remove `onParameterClick` / `onReturnClick` callback injection in `ExplorerCanvas.tsx` `nodesWithCallbacks`
- Simplify `nodesWithCallbacks` in `ExplorerCanvas.tsx` to just pass `flowModel.nodes` directly
- Update highlight logic: nodes in `neighborhoodResult.highlightedIds` get a highlight class; others get dimmed

**Step 4b: Update the rendering files that currently assume path-based tracing**

- In `ExplorerCanvas.tsx`, remove the callback injection layer entirely and pass `flowModel.nodes` straight through
- In `AdaptiveLayout.tsx`, replace `tracePathIds`-based highlight logic with `neighborhoodResult.highlightedIds`
- In `TraceOverlay.tsx`, stop rendering breadcrumb paths; render a compact summary banner like "Showing 12 callers" / "Showing 7 callees" with a clear action instead

**Step 5: Update ContextPanel.tsx**

- Remove the per-parameter trace buttons from the signature section — display signature read-only
- Remove the "Full Call Path" section entirely (it was "trace first param" — misleading)
- Remove the "Dataflow In/Out" section (relies on `parameter_flow` edges that backend never populates)
- Add two honest buttons: **"Show callers"** and **"Show callees"** (not "Trace" — we highlight all reachable nodes, not a specific path)
- When `neighborhoodResult` is active, show a summary: "Highlighting N callers" / "Highlighting N callees" with a clear button
- Keep: Source Code section (wired up in Task 7), Callers list, Callees list, Open in editor

**Step 6: Update ExplorerContextValue type in types.ts**

Replace `traceParameter` and `traceReturn` with `showCallers` and `showCallees`. Replace `traceResult: TraceResult | null` with `neighborhoodResult: NeighborhoodResult | null`. Add `clearHighlight`.

**Step 7: Run tests and fix any broken references**

Run: `cd ui && npx vitest run`

Fix any imports or references that break from the removed `traceParameter`/`traceReturn`/`parameterName`/`TraceResult` types. The existing tests in `hooks.test.ts` and `CodebaseExplorer.test.tsx` that mock `parameterName` data should be updated or removed — they were testing fake behavior. Update `TraceOverlay.tsx` to show a neighborhood summary (e.g. "Showing N callers") instead of a breadcrumb path.

**Step 8: Commit**

```bash
git add ui/src/features/workstation/CodebaseExplorer/
git commit -m "refactor(explorer): replace fake parameter-trace with honest neighborhood highlighting"
```

---

### Task 4: Fix edge labels — show relation and value_preview honestly

**Files:**
- Modify: `ui/src/features/workstation/CodebaseExplorer/AdaptiveLayout.tsx:188-208`

**Step 1: Fix edge label computation**

In `buildFlowModel`, replace the edge label logic:

```ts
// Replace:  label: edge.relation === "contains" ? undefined : edge.relation,
// With:
label: edge.relation === "contains"
  ? undefined
  : edge.valuePreview
    ? `${edge.relation}: ${edge.valuePreview}`
    : edge.relation,
```

**Step 2: Fix edge dedup key to not collapse distinct value_preview**

Replace the dedup key at line 168:

```ts
// Replace:  const key = `${edge.from}::${edge.to}::${edge.relation}`;
// With:
const key = `${edge.from}::${edge.to}::${edge.relation}::${edge.valuePreview ?? ""}`;
```

**Step 3: Run tests**

Run: `cd ui && npx vitest run`

**Step 4: Commit**

```bash
git add ui/src/features/workstation/CodebaseExplorer/AdaptiveLayout.tsx
git commit -m "fix(explorer): show value_preview on edges, dedup by full identity"
```

---

## Phase 3: Web Layout — Graph + Code Side by Side

### Task 5: Replace tabbed web layout with persistent split view

This is the highest-impact UX change. The web workstation currently hides the graph behind a tab. Replace with a CSS grid split: left file tree, center top = code editor, center bottom = graph canvas + context panel. No Allotment dependency needed (that's desktop-only) — pure CSS grid.

**Files:**
- Modify: `ui/src/features/workstation/WorkstationShell.tsx` (web branch only — do NOT touch desktop Allotment path)
- Modify: `ui/src/features/workstation/WorkstationShell.test.tsx` (rewrite HTTP-mode tests for new layout)
- Modify: `ui/src/styles.css`

**Step 1: Rewrite WorkstationShell web layout (web-only — do NOT touch desktop path)**

Replace the tabbed web layout with a persistent split. **Scope: only the HTTP/web branch.** The desktop Allotment path (the `useSplitLayout ? (<Allotment>...</Allotment>)` branch at line 212) MUST remain untouched.

Target web layout:

```
+----------+----------------------------+-----------+
| File     | Code Editor                | Security  |
| Tree     |                            | Overview  |
| (left)   +----------------------------+           |
|          | Graph Canvas + Context     |           |
|          |                            |           |
+----------+----------------------------+-----------+
| Activity Console                                  |
+---------------------------------------------------+
```

Key changes in WorkstationShell:
- **Keep** `useSplitLayout` — it gates the desktop Allotment path and must stay
- **Remove** `useTabbedWebLayout` and `activeMainTab` state — only used in the web branch
- **Replace** the `else` branch (lines 280-391) with the new CSS grid layout
- The desktop path (`useSplitLayout === true`) is **explicitly out of scope** — do not modify it
- In web mode, show SecurityOverviewPanel in a right sidebar (it was only in a tab before)
- Intentionally defer: ChecklistPanel, AuditPlanPanel, ToolbenchPanel, ReviewQueue (per the teammate's recommendation)
- `handleNavigateToSource` should no longer switch tabs — remove the `if (useTabbedWebLayout)` guard, just select the file; code editor updates in-place while graph stays visible

**Step 1b: Rewrite WorkstationShell.test.tsx for the new web layout**

The existing HTTP-mode tests assert tabbed layout behavior that Task 5 removes. Rewrite them:

Replace "uses Code/Graph/Security tabs in http mode" (line 166) with:
```tsx
it("renders code editor and graph side by side in http mode", () => {
  transportKind = "http";
  render(<WorkstationShell sessionId="sess-1" />);

  // Both code editor and graph should be visible simultaneously — no tabs
  expect(screen.getByTestId("codebase-explorer-state")).toBeInTheDocument();
  expect(screen.queryByRole("tab", { name: /^graph$/i })).not.toBeInTheDocument();
});
```

Replace "switches to code tab and forwards line when graph navigation is requested in http mode" (line 180) with:
```tsx
it("navigates to source without losing graph in http mode", () => {
  transportKind = "http";
  render(<WorkstationShell sessionId="sess-1" />);

  // Graph is already visible
  expect(screen.getByTestId("codebase-explorer-state")).toBeInTheDocument();

  fireEvent.click(screen.getByRole("button", { name: /navigate from graph/i }));
  expect(selectFileSpy).toHaveBeenCalledWith("rollup-core/src/lib.rs");

  // Graph should still be visible after navigation — no tab switching
  expect(screen.getByTestId("codebase-explorer-state")).toBeInTheDocument();
  expect(screen.getByTestId("code-editor-state")).toHaveTextContent("none@12");
});
```

**Step 2: Update CSS**

Add a new `.workstation-web-grid` class:

```css
.workstation-web-grid {
  display: grid;
  grid-template-columns: 220px minmax(0, 1fr) 280px;
  grid-template-rows: minmax(0, 1fr);
  min-height: 0;
  flex: 1;
}

.workstation-web-grid .vscode-editor-column {
  display: grid;
  grid-template-rows: minmax(0, 1fr) minmax(240px, 0.45fr);
  min-height: 0;
}
```

Remove the `.workstation-view-tabs`, `.workstation-view-tab`, `.workstation-view-panel` CSS rules — they are no longer used.

**Step 3: Verify "Open in editor" from ContextPanel works**

Clicking "Open in editor" in the graph's ContextPanel should call `onNavigateToSource`, which calls `selectFile` — the code pane updates, the graph stays visible. No tab switching.

**Step 4: Run UI tests**

Run: `cd ui && npx vitest run`

**Step 5: Commit**

```bash
git add ui/src/features/workstation/WorkstationShell.tsx ui/src/features/workstation/WorkstationShell.test.tsx ui/src/styles.css
git commit -m "feat(web-ui): persistent split layout with graph + code side by side"
```

---

## Phase 4: Make Small Repos Explorable by Default

### Task 6: Auto-expand to symbol level for small repos

For repos under the `small` threshold (< 30 files), the default `auto` granularity resolves to `files`, but the overview payload excludes symbols, and file nodes are passive (no drill-down). Fix: when auto-resolved granularity is `files` and the total file count is small, automatically request `full` depth from the backend so symbols are present, and make file nodes expandable.

**Files:**
- Modify: `ui/src/features/workstation/CodebaseExplorer/hooks/useUnifiedGraph.ts`
- Modify: `ui/src/features/workstation/CodebaseExplorer/nodes/FileNode.tsx`
- Modify: `ui/src/features/workstation/CodebaseExplorer/ExplorerContext.tsx`

**Step 1: In useUnifiedGraph, accept a `requestFull` parameter and expose explicit load stage**

Change the initial load from always requesting `overview` to conditionally requesting `full`:

```ts
export function useUnifiedGraph(
  sessionId: string,
  requestFull: boolean
): {
  ...,
  graphDepth: "overview" | "full";
  hasLoadedOverview: boolean;
} 
```

In the `useEffect`, use `requestFull ? "full" : "overview"` as the depth parameter. Similarly in `reload`.

Track two explicit states inside the hook:

- `graphDepth`: the depth of the currently loaded graph (`"overview"` or `"full"`)
- `hasLoadedOverview`: set to `true` only after the first successful overview response resolves

These are not cosmetic. They prevent the bootstrap bug where an empty initial graph could be misread as "small" and immediately request `full` before the overview load has completed.

**Step 2: In ExplorerContext, pass requestFull based on file count from overview**

There's a chicken-and-egg: `resolvedGranularity` depends on `fileCount` which comes from the overview graph. Solve with a two-phase approach:

1. Always request `overview` first (this is the current behavior)
2. After the overview load succeeds, `hasLoadedOverview === true`
3. `ExplorerContext` computes `fileCount` from `graph.nodes.filter(n => n.kind === "file").length` (this already exists at line 42-45)
4. `useAdaptiveThresholds` resolves granularity — if `resolvedGranularity === "files"` (meaning `fileCount < thresholds.small`, i.e. `< 30` by default), and `hasLoadedOverview === true`, and `graphDepth === "overview"`, then pass `requestFull = true` to `useUnifiedGraph`
5. `useUnifiedGraph` detects `requestFull` changed from false to true, and re-requests with `"full"` depth — a single extra request only for genuinely small repos

**Important:** Use the existing `fileCount`-based threshold from `useAdaptiveThresholds` (which counts only `kind === "file"` nodes), NOT a raw `graph.nodes.length < 30` check. Overview node count includes crate and module structure nodes, so a deeply nested small repo could have 30+ total nodes but fewer than 30 files.

**Bootstrap guard:** Do not enable the second request based on an empty graph. The condition must explicitly require `hasLoadedOverview === true`; `graph.nodes.length === 0` is not sufficient.

**Step 3: Make FileNode clickable (trigger focus)**

Update `FileNode.tsx` to show the child count and make it visually indicate it's interactive:

```tsx
import { Handle, Position } from "reactflow";

type FileNodeData = {
  label: string;
  childCount?: number;
};

export function FileNode({ data }: { data: FileNodeData }) {
  return (
    <div className="explorer-file-node">
      <Handle type="target" position={Position.Top} style={{ visibility: "hidden" }} />
      <span className="explorer-file-label">{data.label}</span>
      {data.childCount != null && data.childCount > 0 ? (
        <span className="explorer-file-count">{data.childCount}</span>
      ) : null}
      <Handle type="source" position={Position.Bottom} style={{ visibility: "hidden" }} />
    </div>
  );
}
```

In `ExplorerCanvas.tsx` `handleNodeClick`: clicking a file node should call `ctx.focusNode(node.id)` (it already does for non-cluster nodes).

**Step 4: Add CSS for file node count badge**

```css
.explorer-file-count {
  font-size: 10px;
  color: var(--text-secondary, #94A3B8);
  background: var(--bg-elevated, #334155);
  border-radius: 4px;
  padding: 1px 5px;
  margin-left: 6px;
}
```

**Step 5: Run tests**

Run: `cd ui && npx vitest run`

**Step 6: Commit**

```bash
git add ui/src/features/workstation/CodebaseExplorer/
git commit -m "feat(explorer): auto-expand small repos to symbol level, make file nodes interactive"
```

---

## Phase 5: Wire Real Source Code into Context Panel

### Task 7: Load actual source snippet in ContextPanel

Replace the hardcoded placeholder with a real source code fetch when the user expands the "Source Code" section.

**Files:**
- Modify: `ui/src/features/workstation/CodebaseExplorer/ContextPanel.tsx`
- Modify: `ui/src/features/workstation/CodebaseExplorer/ExplorerContext.tsx` (pass sessionId)
- Modify: `ui/src/features/workstation/CodebaseExplorer/types.ts` (add sessionId to context)

**Step 1: Add sessionId to ExplorerContextValue**

In `types.ts`, add `sessionId: string` to `ExplorerContextValue`.
In `ExplorerContext.tsx`, pass it through from the provider props.

**Step 2: Fetch source in ContextPanel when "Source Code" is expanded (with stale-response guard)**

In a graph-first UI, users click through nodes rapidly. Without cancellation, a slow response for node A can overwrite the snippet for the currently focused node B. Use a request ID ref to discard stale responses:

```tsx
// Inside ContextPanel, after the existing useState for expandedSections:
const { sessionId } = useExplorer();
const [sourceSnippet, setSourceSnippet] = useState<string | null>(null);
const [sourceLoading, setSourceLoading] = useState(false);
const requestIdRef = useRef(0);

// Clear snippet immediately when focused node changes
useEffect(() => {
  setSourceSnippet(null);
}, [focusedNode?.id]);

useEffect(() => {
  if (!expandedSections.has("source") || !focusedNode?.filePath) {
    return;
  }
  const requestId = ++requestIdRef.current;
  setSourceLoading(true);
  void readSourceFile(sessionId, focusedNode.filePath)
    .then((response) => {
      // Discard if user has already moved to a different node
      if (requestId !== requestIdRef.current) return;
      if (focusedNode.line) {
        // Show ~15 lines around the target line
        const lines = response.content.split("\n");
        const start = Math.max(0, focusedNode.line - 8);
        const end = Math.min(lines.length, focusedNode.line + 7);
        setSourceSnippet(
          lines.slice(start, end)
            .map((line, i) => `${start + i + 1} | ${line}`)
            .join("\n")
        );
      } else {
        // Show first 30 lines
        setSourceSnippet(
          response.content.split("\n").slice(0, 30)
            .map((line, i) => `${i + 1} | ${line}`)
            .join("\n")
        );
      }
    })
    .catch(() => {
      if (requestId !== requestIdRef.current) return;
      setSourceSnippet("// Failed to load source");
    })
    .finally(() => {
      if (requestId !== requestIdRef.current) return;
      setSourceLoading(false);
    });
}, [expandedSections, focusedNode?.filePath, focusedNode?.line, focusedNode?.id, sessionId]);
```

Replace the placeholder `<pre>` with:

```tsx
{expandedSections.has("source") ? (
  <div className="explorer-ctx-source-preview">
    {sourceLoading ? (
      <p className="explorer-ctx-source-loading">Loading...</p>
    ) : (
      <pre className="explorer-ctx-code">{sourceSnippet ?? "// No source available"}</pre>
    )}
  </div>
) : null}
```

Import `readSourceFile` from `../../../../ipc/commands`. Add `useRef` to the React import.

**Step 3: Run tests**

Run: `cd ui && npx vitest run`

**Step 4: Commit**

```bash
git add ui/src/features/workstation/CodebaseExplorer/
git commit -m "feat(explorer): wire real source snippet into context panel"
```

---

## Phase 6: Error Handling and Silent Failures

### Task 8: Surface cluster expansion errors and fix search-through-granularity

**Files:**
- Modify: `ui/src/features/workstation/CodebaseExplorer/hooks/useUnifiedGraph.ts:172-175`
- Modify: `ui/src/features/workstation/CodebaseExplorer/ExplorerContext.tsx`

**Step 1: Surface cluster load errors**

In `useUnifiedGraph.ts`, the error handler for cluster expansion (line ~172) silently swallows failures. Add an error state per cluster:

Add a `clusterErrors` state:
```ts
const [clusterErrors, setClusterErrors] = useState<Map<string, string>>(new Map());
```

In the catch block of `expandCluster`:
```ts
(error) => {
  if (generation !== generationRef.current) return;
  setLoadingClusters((previous) => {
    const next = new Set(previous);
    next.delete(clusterId);
    return next;
  });
  setClusterErrors((previous) => {
    const next = new Map(previous);
    next.set(clusterId, error instanceof Error ? error.message : "Failed to load");
    return next;
  });
}
```

Return `clusterErrors` from the hook. Also return:

- `loadedClusters` (already tracked internally)
- `graphDepth` / `hasLoadedOverview` from Task 6

**Step 2: Show error state on ClusterNode**

In `ClusterNode.tsx`, show a red indicator if the cluster failed to expand. Access `clusterErrors` from context.

**Step 3: Fix search — honestly scope to loaded graph and say so**

Current search (`ExplorerContext.tsx:54`) only sees `graph.nodes` — which is the loaded/merged graph. Nodes inside unexpanded clusters are invisible. The plan's original hint ("N matches hidden at current granularity level") only helps when matches are loaded but filtered by visibility — it does NOT help with matches inside lazy clusters.

For MVP, be honest: explicitly scope search to loaded nodes and tell the user. Do not try to infer "fullness" from cluster state alone.

In `ExplorerContext.tsx`, add a `searchHint` string to the context value. Compute it from explicit graph state:

```ts
const hasExpandableClusters = graph.nodes.some(
  (n) => (n.kind === "crate" || n.kind === "module") && !loadedClusters.has(n.id)
);
const searchHint = searchQuery.trim() && graphDepth !== "full" && hasExpandableClusters
  ? "Searching loaded graph only. Expand clusters to search their contents."
  : null;
```

Why this shape:

- `n.kind === "cluster"` is wrong for the current type model; the expandable node kinds are `crate` and `module`
- `loadedClusters` alone is not enough, because a graph loaded at `full` depth may still have empty `loadedClusters`
- `graphDepth === "full"` is the authoritative signal that the entire graph is already loaded

**Step 4: Show the search scope hint in the toolbar**

In `index.tsx` ExplorerToolbar, after the match count badge:
```tsx
{ctx.searchHint ? (
  <span className="explorer-search-hint">{ctx.searchHint}</span>
) : null}
```

**Step 5: Run tests**

Run: `cd ui && npx vitest run`

**Step 6: Commit**

```bash
git add ui/src/features/workstation/CodebaseExplorer/
git commit -m "fix(explorer): surface cluster errors and search-granularity mismatch"
```

---

## Phase 7: Right-Click Context Menu

### Task 9: Add right-click context menu on graph nodes

**Files:**
- Create: `ui/src/features/workstation/CodebaseExplorer/NodeContextMenu.tsx`
- Modify: `ui/src/features/workstation/CodebaseExplorer/ExplorerCanvas.tsx`
- Modify: `ui/src/styles.css`

**Step 1: Create NodeContextMenu component**

A simple positioned menu with 4 actions:
- Show callers (highlight upstream neighborhood)
- Show callees (highlight downstream neighborhood)
- Open in editor (only if filePath exists)
- Focus / Unfocus toggle

```tsx
type NodeContextMenuProps = {
  x: number;
  y: number;
  nodeId: string;
  hasFilePath: boolean;
  isFocused: boolean;
  onShowCallers: () => void;
  onShowCallees: () => void;
  onOpenInEditor: () => void;
  onToggleFocus: () => void;
  onClose: () => void;
};
```

Render as an absolutely-positioned `<div>` with a list of buttons. Close on click-outside or Escape.

**Step 2: Wire into ExplorerCanvas**

Add `onNodeContextMenu` handler to ReactFlow. Store `{ x, y, nodeId }` in state. Render `<NodeContextMenu>` when present. Clear on pane click.

**Step 3: Add CSS**

```css
.explorer-context-menu {
  position: fixed;
  z-index: 100;
  background: var(--bg-surface, #1E293B);
  border: 1px solid var(--border, #475569);
  border-radius: 6px;
  padding: 4px 0;
  min-width: 180px;
  box-shadow: 0 4px 12px rgba(0,0,0,0.4);
}

.explorer-context-menu button {
  display: block;
  width: 100%;
  text-align: left;
  padding: 6px 12px;
  font-size: 12px;
  color: var(--text-primary, #F8FAFC);
  background: none;
  border: none;
  cursor: pointer;
}

.explorer-context-menu button:hover {
  background: var(--bg-elevated, #334155);
}
```

**Step 4: Run tests**

Run: `cd ui && npx vitest run`

**Step 5: Commit**

```bash
git add ui/src/features/workstation/CodebaseExplorer/
git commit -m "feat(explorer): add right-click context menu on graph nodes"
```

---

## Phase 8: Layout Stability

### Task 10: Fix ELK re-layout triggers and fitView after layout

**Files:**
- Modify: `ui/src/features/workstation/CodebaseExplorer/ExplorerCanvas.tsx`

**Step 1: Memoize topology for layout dependency**

The ELK layout should only rerun when the set of node IDs or edge source/target pairs changes — not when callbacks or style classes change. Create a topology fingerprint:

```ts
const topologyKey = useMemo(() => {
  const nodeIds = flowModel.nodes.map((n) => n.id).sort().join(",");
  const edgeIds = flowModel.edges.map((e) => e.id).sort().join(",");
  return `${nodeIds}|${edgeIds}`;
}, [flowModel.nodes, flowModel.edges]);
```

Use `topologyKey` as the dependency for the layout `useEffect`, not the full nodes/edges arrays.

**Step 2: Call fitView after layout completes**

After `setNodes` / `setEdges` in the layout effect, call:
```ts
requestAnimationFrame(() => {
  flowRef.current?.fitView({ padding: 0.16 });
});
```

**Step 3: Separate style updates from layout**

When only highlight classes change (focus/trace/search), update node `className` and edge `style` without triggering ELK. Split into two effects: one for topology (triggers ELK), one for styling (just patches existing nodes/edges in place).

**Step 4: Run tests**

Run: `cd ui && npx vitest run`

**Step 5: Commit**

```bash
git add ui/src/features/workstation/CodebaseExplorer/ExplorerCanvas.tsx
git commit -m "perf(explorer): only re-layout on topology change, fitView after layout"
```

---

## Phase 9: Visual Polish

### Task 11: Style MiniMap and Controls to match design system

**Files:**
- Modify: `ui/src/styles.css`

**Step 1: Override ReactFlow default styles**

```css
.react-flow__minimap {
  background: var(--bg-surface, #1E293B) !important;
  border: 1px solid var(--border, #475569);
  border-radius: 6px;
}

.react-flow__controls {
  background: transparent;
  border: none;
  box-shadow: none;
}

.react-flow__controls-button {
  background: var(--bg-surface, #1E293B) !important;
  border: 1px solid var(--border, #475569) !important;
  color: var(--text-secondary, #94A3B8);
  fill: var(--text-secondary, #94A3B8);
  border-radius: 4px;
  width: 28px;
  height: 28px;
}

.react-flow__controls-button:hover {
  background: var(--bg-elevated, #334155) !important;
  color: var(--text-primary, #F8FAFC);
  fill: var(--text-primary, #F8FAFC);
}
```

**Step 2: Commit**

```bash
git add ui/src/styles.css
git commit -m "style(explorer): match MiniMap and Controls to design system"
```

---

## Summary: Task Dependency Order

```
Phase 1 (Backend)
  Task 1: Fix nested module hierarchy
  Task 2: Assert edge truthfulness

Phase 2 (Remove Lies)
  Task 3: Strip fake parameter-trace, replace with honest neighborhood highlighting
  Task 4: Fix edge labels and dedup

Phase 3 (Layout)
  Task 5: Persistent split view (graph + code side by side)

Phase 4 (Small Repos)
  Task 6: Auto-expand small repos to symbol level

Phase 5 (Source Wiring)
  Task 7: Wire real source snippet into context panel
    Depends on: Task 3 (context panel cleanup), Task 5 (sessionId accessible)

Phase 6 (Error Handling)
  Task 8: Surface cluster errors, fix search hints
    Depends on: Task 6 (granularity changes)

Phase 7 (Interaction)
  Task 9: Right-click context menu
    Depends on: Task 3 (neighborhood highlight API is finalized)

Phase 8 (Performance)
  Task 10: Fix ELK re-layout and fitView
    Depends on: Task 3 (callback changes settled), Task 4 (edge changes settled)

Phase 9 (Polish)
  Task 11: Style MiniMap/Controls
    No dependencies, can be done anytime
```

Tasks 1-2 are independent and can run in parallel.
Tasks 3-4 should be executed sequentially, not in parallel. After Task 3's write set was made explicit, both tasks now touch `AdaptiveLayout.tsx`, and Task 4 also depends on the post-Task-3 neighborhood highlight model.
Task 5 is the single biggest UX impact — prioritize after Phase 2.
Tasks 6-11 can be parallelized in pairs after Task 5 lands.

---

## What This Plan Explicitly Does NOT Do

- No new agent invocation UI (Mode 1 from design doc) — that needs a backend LLM integration endpoint first
- No suspicion markers — requires a persistence model that doesn't exist yet
- No finding lifecycle — ditto
- No Solidity integration — explicitly deferred per user request
- No review queue, audit plan, toolbench in web mode — deferred per teammate recommendation
- No collaboration features — post-MVP
- No report generation — post-MVP

The goal is: load a Rust/Cairo/Circom repo, see a truthful graph, click through it, read source, highlight caller/callee neighborhoods, and have code + graph visible at all times. That's the demo.
