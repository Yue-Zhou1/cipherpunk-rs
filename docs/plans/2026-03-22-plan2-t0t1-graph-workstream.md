# Plan 2 — T0/T1: Graph Workstream

> **For Agent:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Stabilize the graph UI path so it does not crash or mislead, then turn the ProjectIR-powered graph into the primary investigation surface — clickable, finding-annotated, searchable, and useful for security review.

**Architecture:** Item A is a bug-fix and hardening pass scoped to the graph rendering path only. Item B adds interactive features on the stable foundation. Both touch the same files; execute sequentially (A before B).

**Tech Stack:** React 18, TypeScript, Vite, Cytoscape.js, ReactFlow + ELK.js, Tailwind CSS, Axum web server, Tauri IPC, Rust session manager.

**Depends on:** Plan 1A (engine outcomes and coverage report) — the graph UI will render engine status and coverage warnings.

---

## Planning Conventions

- Recommended execution context: a dedicated git worktree.
- Preserve existing Tauri and HTTP transport modes. Test both after each task.
- Use `#[serde(default)]` and TypeScript optional fields for additive wire-format changes.
- Do not introduce new npm dependencies unless strictly necessary.
- Graph rendering must not depend on findings/candidates existing; overlays are optional augmentation on top of a renderable graph.
- Deterministic engines and ProjectIR remain the source of truth for audit analysis; AI layers may assist planning, explanation, and recovery, but do not redefine deterministic results.
- Every task must land with tests (Vitest for UI, Rust integration for backend).

## Release Gates

This plan is complete only when all of the following are true:

- All three graph lenses (file, feature, dataflow) render in both Tauri and HTTP modes without crashing.
- Empty states (no ProjectIR built) show a useful placeholder, not a blank or error.
- Graph nodes are clickable — clicking a file/symbol node navigates to the source in CodeEditorPane.
- Findings/candidates are overlaid on graph nodes with severity color-coding when present; when absent, the graph still renders with no warning-level failure.
- The symbol graph lens is available as a fourth lens option.
- Graph search filters nodes by label substring.
- `cd ui && npm run build` and `cd ui && npm test` pass.
- `cargo test -p session-manager` and `cargo test -p web-server` pass.

## Explicit Non-Goals

- Full graph editing (add/remove nodes).
- Neighborhood expansion via separate API (too complex for this plan; double-click is deferred).
- Graph layout persistence across sessions.
- Mobile-responsive graph layout.

---

## Item A: UI Stabilization for Graph Delivery

### Context

The current branch is `fix-ui`. Recent commits (`b80a16f`, `119e465`, `5706a31`) address split-pane layout and code viewer formatting. The graph components have several fragility points: Cytoscape containers can render with zero dimensions, ELK layout can fail on edge cases, HTTP transport may not construct graph URLs correctly, and there is no handling for the pre-ProjectIR state.

### Tasks

#### Task 1: Fix Cytoscape container zero-dimension rendering

**File:** `ui/src/features/workstation/GraphLensCytoscape.tsx`

Problem: If the Cytoscape container div renders before its parent has non-zero dimensions (common during layout transitions), Cytoscape initializes with a 0×0 canvas and never recovers.

Changes:
1. Add explicit `min-height: 300px` and `min-width: 200px` to the container div's style.
2. Add a `ResizeObserver` that calls `cyRef.current?.resize()` and `cyRef.current?.fit()` when the container dimensions change.
3. Guard the Cytoscape initialization: if `containerRef.current.offsetWidth === 0`, defer initialization via `requestAnimationFrame` retry (max 5 retries).

**Test:** Vitest: render `GraphLensCytoscape` inside a container with `display: none`, then show it. Verify no error thrown and `cyRef` initializes after container becomes visible.

---

#### Task 2: Handle empty graph state with placeholder

**Files:** `ui/src/features/workstation/GraphLensCytoscape.tsx`, `ui/src/features/workstation/GraphLensReactFlow.tsx`

Problem: When no ProjectIR exists yet (session just created, `BuildProjectIr` job hasn't run), the graph fetch returns an error or empty data. Currently this shows a blank area or an uncaught error.

Changes:
1. In the graph data fetch effect, catch errors and set an `error` state.
2. When `graph` is null and `isLoading` is false and `error` is non-null, render a placeholder:
   ```tsx
   <div className="flex items-center justify-center h-full text-gray-500">
     <div className="text-center">
       <p className="text-lg font-medium">No graph data available</p>
       <p className="text-sm mt-1">Run the BuildProjectIr job to generate the code graph.</p>
     </div>
   </div>
   ```
3. When `graph` is non-null but `graph.nodes.length === 0`, show a different placeholder: "Graph is empty — no source files found in the selected scope."

**Test:** Vitest: render graph component with mock IPC that rejects. Verify placeholder text visible.

---

#### Task 3: Add try/catch around ELK layout

**File:** `ui/src/features/workstation/GraphLensReactFlow.tsx`

Problem: The ELK layout algorithm (`layered`) can throw on cyclic graphs, disconnected components with certain configurations, or when nodes have invalid dimensions.

Changes:
1. Wrap the `elk.layout(graph)` call in try/catch.
2. On ELK failure, fall back to a simple grid layout: arrange nodes in rows of 4, each 300px wide, 80px tall, with 20px gaps.
3. Log the ELK error to console: `console.warn("ELK layout failed, using grid fallback:", error)`.
4. Set a state flag `layoutFallback: boolean` and show a small info badge: "Using simplified layout" when true.

**Test:** Vitest: mock ELK to throw. Verify nodes still render in grid positions. Verify fallback badge visible.

---

#### Task 4: Verify HTTP transport for graph endpoints

**File:** `ui/src/ipc/transport.ts`

Verify and fix if needed:
1. The graph endpoint URL is constructed as `${baseUrl}/api/sessions/${sessionId}/graphs/${lens}`.
2. The `include_values` query parameter for dataflow is appended correctly: `?include_values=true`.
3. No double `/api/` prefix (check if `baseUrl` already includes `/api`).

**File:** `ui/src/ipc/commands.ts`

Verify `loadFileGraph`, `loadFeatureGraph`, `loadDataflowGraph` functions:
1. Add 10-second timeout via `AbortController` on the fetch call.
2. On timeout, return the fixture data in development mode or throw a user-friendly error in production.
3. Handle non-200 responses (parse error envelope, show message).

**Test:** Vitest: mock fetch with delayed response > 10s. Verify timeout error surfaced. Mock fetch with 404 response. Verify error message shown.

---

#### Task 5: Verify graph panel has render space in HTTP mode

**File:** `ui/src/features/workstation/WorkstationShell.tsx`

After commit `b80a16f` disabled split-pane in HTTP mode, verify:
1. The graph panel has a dedicated render area. If it's sharing space with the code editor, they need to be in a tab switcher or the graph needs its own full-width section.
2. If tabs are needed, add a simple tab bar: "Code" | "Graph" | "Security" above the main content area.
3. The graph panel should get at least 500px × 400px when visible.

This task is an audit + fix — the exact change depends on what the current layout looks like after the split-pane disable.

---

#### Task 6: Handle missing ProjectIR in web server

**File:** `crates/apps/web-server/src/lib.rs`

In the `load_graph` handler, when the session manager returns an error because ProjectIR hasn't been built yet:
1. Return HTTP 404 with `{ "error": { "code": "PROJECT_IR_NOT_BUILT", "message": "ProjectIR has not been built for this session. Run BuildProjectIr first.", "status": 404 } }`.
2. Do not return 500 — this is an expected state, not a server error.

**File:** `crates/services/session-manager/src/state.rs`

In `load_file_graph`, `load_feature_graph`, `load_dataflow_graph` — if `load_or_build_project_ir` fails because no source has been resolved, map the error to a `SessionManagerError::NotFound` rather than `Internal`.

**Test:** Integration: create a session without resolving source, call graph endpoint. Verify 404 response with correct error code.

---

#### Task 7: Smoke test checklist

Manual verification (documented here for the implementer):

1. Start web server (`./start-web-http.sh`).
2. Create a new session via the wizard.
3. Navigate to graph tab **before** running BuildProjectIr → verify placeholder shown, no crash.
4. Run BuildProjectIr job.
5. Navigate to graph tab → verify file graph renders with nodes and edges.
6. Switch to feature graph → renders.
7. Switch to dataflow graph → renders (with redacted values by default).
8. Toggle "Include values" → dataflow edges show value previews.
9. Repeat steps 2-8 in Tauri mode if available.

---

## Item B: ProjectIR-Powered Interactive Graph UI

### Context

Graph APIs exist and serve data. UI components render static graphs. But the graph is currently passive — no click-to-navigate, no finding overlay, no search, no symbol graph lens. This item transforms the graph from a visualization into an investigation tool.

### Tasks

#### Task 8: Click-to-navigate from graph nodes to source

**File:** `ui/src/features/workstation/GraphLensCytoscape.tsx`

Add a Cytoscape `tap` event handler on nodes:

```typescript
cy.on("tap", "node", (event) => {
  const node = event.target;
  const filePath = node.data("filePath");
  const kind = node.data("kind") as string;

  if (!filePath || !onNavigateToSource) return;

  // Extract line number from symbol node IDs: "symbol::/path/file.rs::function_name"
  // The symbol graph nodes have line info, file graph nodes don't.
  let line: number | undefined;
  const id = node.data("id") as string;
  if (kind.startsWith("function") || kind.startsWith("circom_template")) {
    // Line is available in the SymbolNode data — pass it through if available
    line = node.data("line");
  }

  onNavigateToSource(filePath, line);
});
```

Add `cursor: pointer` CSS for clickable nodes (nodes with `filePath`).

**File:** `ui/src/features/workstation/GraphLensReactFlow.tsx`

Add `onClick` handler to the custom node component:

```typescript
const onNodeClick = useCallback((_: React.MouseEvent, node: FlowNode) => {
  if (!node.data.filePath || !onNavigateToSource) return;
  onNavigateToSource(node.data.filePath, node.data.line);
}, [onNavigateToSource]);
```

Pass to ReactFlow: `<ReactFlow onNodeClick={onNodeClick} ... />`.

**File:** `ui/src/features/workstation/WorkstationShell.tsx`

Wire the `onNavigateToSource` callback from graph panel to CodeEditorPane:
1. Set the active file path in workstation state.
2. If a line number is provided, scroll the code editor to that line.
3. If the workstation uses tabs, switch to the "Code" tab after navigation.

**Test:** Vitest: render graph with mock node data containing `filePath`. Simulate click. Verify `onNavigateToSource` callback called with correct path and line.

---

#### Task 9: Add finding_count and max_severity to graph node response

**File:** `crates/services/session-manager/src/workflow.rs`

Extend `ProjectGraphNodeResponse`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectGraphNodeResponse {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub file_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finding_count: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_severity: Option<String>,
}
```

**File:** `crates/services/session-manager/src/state.rs`

Add helper function:

```rust
fn annotate_graph_with_findings(
    nodes: &mut [ProjectGraphNodeResponse],
    records: &[AuditRecord],
) {
    // Build a map: file_path → (count, max_severity) from records that have locations.
    // For each node with a file_path, look up in the map and populate finding_count + max_severity.
    // Severity ordering: Critical > High > Medium > Low > Observation.
}
```

Call `annotate_graph_with_findings` in `file_graph_response`, `feature_graph_response`, `dataflow_graph_response`, and the new `symbol_graph_response` (Task 11) — after building the base response, before returning.

Load records from session store (`list_records(session_id, Some("finding"))` + `list_records(session_id, Some("candidate"))`).

**Test:** Rust unit test: create mock nodes and records with overlapping file paths. Verify `finding_count` and `max_severity` populated correctly. Verify nodes without matching records have `None`.

---

#### Task 10: Render finding severity badges on graph nodes

**File:** `ui/src/features/workstation/GraphLensCytoscape.tsx`

Update the Cytoscape element builder to add severity classes:

```typescript
const severityClass = node.maxSeverity?.toLowerCase();
const classes: string[] = [];
if (selectedNodeIds?.has(node.id)) classes.push("selected");
if (focusSymbolName && (node.label.includes(focusSymbolName) || node.id.includes(focusSymbolName))) {
  classes.push("focus-match");
}
if (severityClass) classes.push(`severity-${severityClass}`);
if (node.findingCount && node.findingCount > 0) classes.push("has-findings");
```

Add Cytoscape stylesheet entries:

```typescript
{ selector: ".severity-critical", style: { "border-color": "#dc2626", "border-width": 3 } },
{ selector: ".severity-high", style: { "border-color": "#ea580c", "border-width": 3 } },
{ selector: ".severity-medium", style: { "border-color": "#ca8a04", "border-width": 2 } },
{ selector: ".severity-low", style: { "border-color": "#2563eb", "border-width": 2 } },
{ selector: ".has-findings", style: { "background-opacity": 0.15 } },
```

**File:** `ui/src/features/workstation/GraphLensReactFlow.tsx`

Update the custom node component to render a severity badge:

```tsx
{node.data.findingCount > 0 && (
  <span className={`absolute -top-2 -right-2 w-5 h-5 rounded-full text-xs text-white flex items-center justify-center ${severityBgColor(node.data.maxSeverity)}`}>
    {node.data.findingCount}
  </span>
)}
```

Where `severityBgColor` maps: critical→`bg-red-600`, high→`bg-orange-600`, medium→`bg-yellow-600`, low→`bg-blue-600`.

**File:** `ui/src/ipc/commands.ts`

Update TypeScript types:

```typescript
export type ProjectGraphNode = {
  id: string;
  label: string;
  kind: string;
  filePath?: string;
  findingCount?: number;
  maxSeverity?: string;
};
```

**Test:** Vitest: render graph with nodes that have `findingCount: 3, maxSeverity: "critical"`. Verify severity class applied. Snapshot test for badge rendering.

---

#### Task 11: Add symbol graph lens

**File:** `crates/services/session-manager/src/state.rs`

Add method:

```rust
pub async fn load_symbol_graph(
    &mut self,
    session_id: &str,
) -> Result<ProjectGraphResponse> {
    let ir = self.load_or_build_project_ir(session_id).await?;
    Ok(symbol_graph_response(session_id, &ir))
}

fn symbol_graph_response(session_id: &str, ir: &ProjectIr) -> ProjectGraphResponse {
    let nodes = ir.symbol_graph.nodes.iter().map(|n| {
        ProjectGraphNodeResponse {
            id: n.id.clone(),
            label: n.name.clone(),
            kind: n.kind.clone(),
            file_path: Some(n.file.to_string_lossy().to_string()),
            finding_count: None,
            max_severity: None,
        }
    }).collect();
    let edges = ir.symbol_graph.edges.iter().enumerate().map(|(i, e)| {
        ProjectGraphEdgeResponse {
            from: e.from.clone(),
            to: e.to.clone(),
            relation: e.relation.clone(),
            value_preview: None,
        }
    }).collect();
    ProjectGraphResponse {
        session_id: session_id.to_string(),
        lens: "symbol".to_string(),
        redacted_values: true,
        nodes,
        edges,
    }
}
```

**File:** `crates/services/session-manager/src/lib.rs`

Add `pub async fn load_symbol_graph(&self, session_id: &str)` delegation method.

**File:** `crates/apps/web-server/src/lib.rs`

In `load_graph` handler, add `"symbol"` to the lens match:

```rust
"symbol" => manager.load_symbol_graph(&session_id).await,
```

**Files:** `ui/src/features/workstation/GraphLensCytoscape.tsx`, `ui/src/features/workstation/GraphLensReactFlow.tsx`

Add to `LENS_OPTIONS`:

```typescript
{ kind: "symbol", label: "Symbol Graph" },
```

Finding annotation for the symbol graph is optional augmentation layered after Task 9. The core lens must work even when there are no finding/candidate records yet.

Update `GraphLensKind` type:

```typescript
export type GraphLensKind = "file" | "feature" | "dataflow" | "symbol";
```

Add IPC function:

```typescript
export async function loadSymbolGraph(sessionId: string): Promise<ProjectGraphResponse> {
  return loadGraph(sessionId, "symbol");
}
```

**Test:** Backend: build ProjectIR for a test workspace, call `load_symbol_graph`. Verify nodes are functions, edges are call relationships. Frontend: render with symbol lens selected, verify nodes show function names.

---

#### Task 12: Add graph search/filter

**Files:** `ui/src/features/workstation/GraphLensCytoscape.tsx`, `ui/src/features/workstation/GraphLensReactFlow.tsx`

Add a search input above the graph:

```tsx
const [searchQuery, setSearchQuery] = useState("");

const matchingNodeIds = useMemo(() => {
  if (!searchQuery.trim() || !graph) return null;
  const q = searchQuery.toLowerCase();
  return new Set(
    graph.nodes
      .filter(n => n.label.toLowerCase().includes(q) || n.id.toLowerCase().includes(q))
      .map(n => n.id)
  );
}, [searchQuery, graph]);
```

**Cytoscape implementation:**
- When `matchingNodeIds` is non-null, add `"search-dimmed"` class to non-matching nodes.
- Stylesheet: `{ selector: ".search-dimmed", style: { opacity: 0.15 } }`.
- Show match count: `"{matchingNodeIds.size} of {graph.nodes.length} nodes"`.

**ReactFlow implementation:**
- When `matchingNodeIds` is non-null, set `style.opacity = 0.15` on non-matching nodes.
- On Enter key or clicking a match, fit the viewport to matching nodes.

Search input UI:

```tsx
<div className="flex items-center gap-2 px-3 py-2 border-b border-gray-200">
  <input
    type="text"
    placeholder="Search nodes..."
    value={searchQuery}
    onChange={(e) => setSearchQuery(e.target.value)}
    className="flex-1 text-sm border border-gray-300 rounded px-2 py-1"
  />
  {matchingNodeIds && (
    <span className="text-xs text-gray-500">
      {matchingNodeIds.size} / {graph?.nodes.length ?? 0}
    </span>
  )}
</div>
```

**Test:** Vitest: render graph with 5 nodes, type search query matching 2 nodes. Verify 2 nodes have normal opacity, 3 are dimmed. Clear search → all nodes normal.

---

#### Task 13: Graph panel toolbar

**Files:** `ui/src/features/workstation/GraphLensCytoscape.tsx`, `ui/src/features/workstation/GraphLensReactFlow.tsx`

Consolidate the controls into a compact toolbar:

```tsx
<div className="flex items-center gap-2 px-3 py-2 border-b border-gray-200 bg-gray-50">
  {/* Lens selector */}
  <select value={lens} onChange={(e) => setLens(e.target.value as GraphLensKind)}
    className="text-sm border border-gray-300 rounded px-2 py-1">
    {LENS_OPTIONS.map(opt => (
      <option key={opt.kind} value={opt.kind}>{opt.label}</option>
    ))}
  </select>

  {/* Search input */}
  <input type="text" placeholder="Search..." value={searchQuery}
    onChange={(e) => setSearchQuery(e.target.value)}
    className="flex-1 text-sm border border-gray-300 rounded px-2 py-1 max-w-xs" />

  {/* Include values toggle — dataflow only */}
  {lens === "dataflow" && (
    <label className="flex items-center gap-1 text-xs text-gray-600">
      <input type="checkbox" checked={includeValues}
        onChange={(e) => setIncludeValues(e.target.checked)} />
      Values
    </label>
  )}

  {/* Search match count */}
  {matchingNodeIds && (
    <span className="text-xs text-gray-500">{matchingNodeIds.size} matches</span>
  )}

  {/* Fit to screen button */}
  <button onClick={() => fitToScreen()} className="text-xs px-2 py-1 border rounded"
    title="Fit to screen">Fit</button>
</div>
```

The `fitToScreen` function calls `cyRef.current?.fit()` (Cytoscape) or `reactFlowInstance.fitView()` (ReactFlow).

---

## Dependency Map

```
Task 1  (Cytoscape resize)     ← no deps
Task 2  (empty state)          ← no deps
Task 3  (ELK fallback)         ← no deps
Task 4  (HTTP transport)       ← no deps
Task 5  (layout space)         ← no deps
Task 6  (web server 404)       ← no deps
Task 7  (smoke test)           ← Tasks 1-6

Task 8  (click-to-navigate)    ← Tasks 1-6 (stable base)
Task 9  (finding annotation)   ← no deps (backend)
Task 10 (severity badges)      ← Task 9
Task 11a (symbol graph lens core) ← Tasks 1-6 (stable base)
Task 11b (symbol graph annotation) ← Task 9, Task 11a
Task 12 (search/filter)        ← Tasks 1-6 (stable base)
Task 13 (toolbar)              ← Tasks 8, 10, 11a, 11b, 12
```

Tasks 1-6 (Item A) can be done in any order. Task 7 validates them all.
Tasks 8-12 (Item B) can mostly be parallelized after Item A is complete. Task 13 integrates them.
