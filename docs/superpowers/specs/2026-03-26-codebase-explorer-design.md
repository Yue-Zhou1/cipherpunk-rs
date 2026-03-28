# Codebase Explorer — Design Spec

> Interactive web-UI for auditors to explore target codebase structure, trace data flow through files and functions, and investigate connections. A codebase comprehension playground — not a findings dashboard.

## Problem

Experienced auditors need to quickly build a mental model of an unfamiliar codebase before they can assess vulnerabilities. They need to understand: how files connect, how functions call each other, where input arguments originate, where outputs flow, what types are involved, and what the blast radius of a given function is.

The current `GraphLensReactFlow.tsx` shows isolated graph views (file, symbol, feature, dataflow) as passive visualizations. It lacks:
- Focus+context interaction (spotlight a node, dim the rest)
- Parameter-level tracing (click a function argument to see where it comes from)
- Adaptive overview (auto-adjust granularity to codebase size)
- Progressive detail loading (start minimal, load heavy analysis on demand)
- Unified view across graph types

## Approach

**Modular rewrite** — new `CodebaseExplorer/` component directory replacing both `GraphLensReactFlow.tsx` and `GraphLensCytoscape.tsx`. Uses ReactFlow + ELK for graph rendering (proven libraries already in the project), with clean component and hook separation for the new interaction model.

**Web-first** — targets the browser, served as static files. No Tauri dependency. Phase 1 uses fixture/mock data for a working interactive demo. API integration deferred to Phase 2.

## Phasing

| Phase | Deliverable | Data Source |
|---|---|---|
| **Phase 1** (this spec) | Working interactive web-UI demo with full interaction model | Fixture/mock graph data |
| **Phase 2** (future) | API endpoints wired to project-ir | Axum REST API |
| **Phase 3** (future) | Advanced features (collaborative sharing, etc.) | Real backend |

## Component Architecture

```
ui/src/features/workstation/CodebaseExplorer/
├── index.tsx                     Main export, wires components + hooks
├── ExplorerCanvas.tsx            ReactFlow wrapper — graph rendering, pan/zoom
├── ContextPanel.tsx              Right panel — node identity, signature, on-demand sections
├── TraceOverlay.tsx              Parameter tracing — highlights trace paths on canvas
├── AdaptiveLayout.tsx            Wraps ExplorerCanvas — transforms graph into clustered/flat view based on granularity
├── nodes/
│   ├── ClusterNode.tsx           Crate/module cluster node — expandable
│   ├── FileNode.tsx              File-level node
│   └── SymbolNode.tsx            Function/trait node with clickable parameters
├── hooks/
│   ├── useFocusContext.ts        Focus+dim state — spotlighted node, neighbor computation
│   ├── useTrace.ts               Parameter tracing — origin/destination path computation
│   ├── useDepthControl.ts        Depth slider state (1-5+, default 2)
│   ├── useAdaptiveThresholds.ts  Configurable size thresholds for overview granularity
│   └── useUnifiedGraph.ts        Merges graph data into single model (fixtures in Phase 1)
├── fixtures/
│   └── mockGraph.ts              Realistic mock graph data for demo
└── types.ts                      Shared types
```

## Data Model

The explorer uses its own internal type system (`ExplorerGraph`) that extends the existing `ProjectGraphResponse` shape with signature and parameter-level data. In Phase 1 this is populated from fixtures; in Phase 2 `useUnifiedGraph` maps API responses into this model.

### ExplorerNode

```ts
type ExplorerNode = {
  id: string;                    // e.g., "symbol:src/crypto/sig.rs::verify_signature"
  label: string;                 // display name
  kind: "crate" | "module" | "file" | "function" | "trait_impl_method" | "macro_call";
  filePath?: string;
  line?: number;
  signature?: FunctionSignature; // only for function/trait_impl_method kinds
};

type FunctionSignature = {
  parameters: ParameterInfo[];
  returnType?: string;           // e.g., "Result<bool>"
};

type ParameterInfo = {
  name: string;                  // e.g., "msg"
  typeAnnotation?: string;       // e.g., "&[u8]"
  position: number;              // 0-indexed parameter position
};
```

### ExplorerEdge

```ts
type ExplorerEdge = {
  from: string;                  // source node ID
  to: string;                    // target node ID
  relation: "calls" | "contains" | "parameter_flow" | "return_flow" | "cfg";
  parameterName?: string;        // for parameter_flow: which parameter this edge feeds into
  parameterPosition?: number;    // for parameter_flow: position index of the target parameter
  valuePreview?: string;         // optional, redacted by default
};
```

### ExplorerGraph

```ts
type ExplorerGraph = {
  nodes: ExplorerNode[];
  edges: ExplorerEdge[];
};
```

### Mapping from ProjectGraphResponse (Phase 2)

`useUnifiedGraph` will merge the 4 existing graph responses into `ExplorerGraph`:
- `loadFileGraph()` → nodes with kind `file`, grouped into `module`/`crate` container nodes
- `loadSymbolGraph()` → nodes with kind `function`/`trait_impl_method`/`macro_call`, plus `calls`/`contains` edges
- `loadDataflowGraph()` → `parameter_flow`/`return_flow` edges with `parameterName` extracted from the dataflow node labels
- `loadFeatureGraph()` → `cfg` edges as annotations

Fields not present in the current `ProjectGraphResponse` (specifically `signature`, `parameterName`, `parameterPosition`) will require either extending the backend types in Phase 2 or deriving them from existing data (e.g., parsing the symbol node ID format or the semantic analysis output).

## Interaction Model

### State Machine

The explorer has 3 states:

```
OVERVIEW ──click node──► FOCUS ──click parameter──► TRACE
    ▲                      │  ▲                       │
    └──── Esc/click empty──┘  └── click diff node ────┘
                               └── Esc/click empty ───┘
```

### OVERVIEW State

- All nodes rendered at equal opacity
- Adaptive granularity auto-selected based on codebase size
- Click a cluster → expand to show children (local expansion only)
- Click a leaf node (file or function) → transition to FOCUS
- Toolbar: granularity override dropdown (Auto / Files / Modules / Crates)
- Toolbar: search input with match count, dims non-matching nodes

### FOCUS State

- Clicked node highlighted green (primary)
- Upstream neighbors (callers) highlighted blue, up to N hops
- Downstream neighbors (callees) highlighted orange, up to N hops
- All other nodes dimmed to ~20% opacity, remain visible for orientation
- Depth slider visible in toolbar (range 1-5+, default 2)
- Context panel appears on right side
- Click a different node → refocus on that node (stay in FOCUS)
- Click empty canvas or Esc → back to OVERVIEW

### TRACE State

- Entered by clicking a parameter in the context panel signature display
- Click a parameter (e.g., `msg: &[u8]`) → trace backwards: where does this value originate?
- Click the return type → trace forwards: where does this output flow?
- Trace path nodes highlighted in purple, edges animated
- Context panel updates to show trace as clickable breadcrumb list
- Clicking any breadcrumb node → refocuses graph on that node
- Click empty canvas or Esc → back to FOCUS on the same node

### Trace Algorithm

The `useTrace` hook computes parameter origin/destination paths using BFS on the `ExplorerGraph`:

**Parameter origin trace** (clicking a parameter):
1. Start from the focused node
2. Filter edges to `parameter_flow` where `parameterName` matches the clicked parameter
3. BFS backwards through `parameter_flow` and `calls` edges, collecting the path
4. Stop at nodes with no further upstream `parameter_flow` edges (origin found)
5. Return ordered path: `[origin, ..., intermediate, ..., focused_node]`

**Return destination trace** (clicking return type):
1. Start from the focused node
2. Follow `return_flow` edges forward
3. BFS through `return_flow` and `calls` edges, collecting the path
4. Stop at leaf nodes (no further downstream flow)
5. Return ordered path: `[focused_node, ..., intermediate, ..., leaf]`

**Dead end handling:** If no trace path exists (parameter is a literal, or no flow edges), the context panel shows "No upstream flow found — value may be constructed locally" and the canvas stays in FOCUS state (does not transition to TRACE).

## Adaptive Overview

Auto-selects initial granularity based on file count:

| Codebase Size | View | Nodes Shown |
|---|---|---|
| Small (< threshold_small files) | File-level | Every source file, grouped by directory |
| Medium (threshold_small – threshold_large) | Module clusters | Directories as collapsible clusters |
| Large (> threshold_large files) | Crate clusters | Crates as single nodes, expandable |

**Default thresholds:** `threshold_small = 30`, `threshold_large = 150`

**Configurable:** Both thresholds adjustable via settings object. Stored per-session.

**Manual override:** Toolbar dropdown with options: Auto, Files, Modules, Crates.

**Expand/collapse:** Click a cluster → expands to show children. Only the clicked cluster expands; others stay collapsed.

## Context Panel

Appears on right side of split-pane when in FOCUS or TRACE state.

### Always Visible (instant, no computation)

- **Node identity**: function name, file path, line number, containing crate/module
- **Function signature**: full typed signature with each parameter and return type as individually clickable elements
- **Connection counts**: N callers, N callees (at current depth)

### On-Demand Sections (collapsed by default, load when clicked)

| Section | Content | Load Trigger |
|---|---|---|
| Source Code | Function body snippet | Click "Load" → fetch source |
| Dataflow In/Out | Per-parameter: where the value originates. Per-return: where it flows. | Click "Load" → graph traversal |
| Full Call Path | Complete path from entry points to this function (or to leaf calls) | Click "Trace" → BFS computation |
| Callers List | All upstream callers with file:line, clickable to refocus | Click "Expand" |
| Callees List | All downstream callees with file:line, clickable to refocus | Click "Expand" |

### Parameter Click Behavior

Clicking a parameter in the signature simultaneously:
1. Updates context panel to show trace path as clickable breadcrumb list
2. Transitions canvas to TRACE state, highlighting path nodes and edges

## Visual Design

### Color Semantics

| Element | Color | Usage |
|---|---|---|
| Focused node | Green (#22c55e) | Currently selected node |
| Upstream (callers) | Blue (#3b82f6) | Nodes that call into the focused node |
| Downstream (callees) | Orange (#f97316) | Nodes called by the focused node |
| Trace path | Purple (#8b5cf6) | Nodes along a parameter trace |
| Dimmed nodes | 20% opacity | Not in current focus neighborhood |
| Cluster borders | Slate (#334155) | Module/crate grouping |

### Node Rendering

- **ClusterNode**: Rounded rectangle with module/crate name, child count badge, expand/collapse indicator
- **FileNode**: Compact rectangle with filename, language icon
- **SymbolNode**: Taller card showing function name + signature. Parameters rendered as individual `<span>` elements with hover highlight and click handler. Return type also clickable.

### Depth Indicator

Visible in toolbar during FOCUS/TRACE states:
- Slider or `[-]` / `[+]` buttons with current depth number
- Range: 1 to 5+
- Default: 2

### Canvas Controls

- ReactFlow built-in: pan, zoom, minimap, fit-to-view
- Search input with match count
- Granularity dropdown
- Depth slider

## Fixture Data (Phase 1)

The demo uses realistic mock data representing a ~50 file crypto project:

- ~50 file nodes across 5 modules/crates
- ~80 symbol nodes (functions, trait impls)
- ~120 edges (calls, parameter_flow, return_flow, contains)
- Realistic function signatures with typed parameters
- Cross-module call chains (entry point → validation → crypto → output)

The fixture data uses the `ExplorerGraph` type defined in the Data Model section. In Phase 2, `useUnifiedGraph` will map `ProjectGraphResponse` API responses into this same shape, so swapping data sources requires only changing the hook's data fetching — all downstream components remain unchanged.

Multiple fixture datasets of different sizes (small: ~15 files, medium: ~50 files, large: ~200 files) allow testing all three adaptive overview tiers. The default demo loads the medium dataset.

## State Management

All explorer state is coordinated through a single `ExplorerProvider` React context wrapping the `CodebaseExplorer` subtree. The hooks read from and write to this shared context:

```
<ExplorerProvider>          ← owns all state
  ├── ExplorerCanvas        ← reads: graph, focus, trace, depth
  ├── ContextPanel          ← reads: focused node, trace path; writes: trace trigger
  ├── TraceOverlay          ← reads: trace path
  └── Toolbar               ← reads/writes: depth, granularity, search
</ExplorerProvider>
```

Each hook encapsulates its domain logic but shares state through the context:
- `useUnifiedGraph` → provides `ExplorerGraph`
- `useFocusContext` → provides focused node ID, neighbor sets, dim set
- `useTrace` → provides trace path, trace state
- `useDepthControl` → provides depth value
- `useAdaptiveThresholds` → provides current granularity level

No external state library needed. The context + hooks pattern is sufficient for this component tree.

## Integration

### What This Replaces

- `ui/src/features/workstation/GraphLensReactFlow.tsx` — removed
- `ui/src/features/workstation/GraphLensCytoscape.tsx` — removed
- `ui/src/features/workstation/GraphLens.tsx` (wrapper/router) — removed
- References in `WorkstationShell.tsx` updated to render `CodebaseExplorer` in the "Graph" tab

### Props Interface

```ts
type CodebaseExplorerProps = {
  sessionId: string;
  onNavigateToSource?: (filePath: string, line?: number) => void;
};
```

The existing `GraphLensProps` fields `selectedNodeIds` and `focusSymbolName` are no longer needed — focus is managed internally by the explorer's own state machine. The `react-cytoscapejs` and `cytoscape` packages can be removed from `package.json` after migration.

## Search Behavior Across States

- **OVERVIEW**: Search dims non-matching nodes, shows match count. Standard behavior.
- **FOCUS**: Search filters within the visible neighborhood. Nodes outside the focus neighborhood remain dimmed regardless of match. Matching nodes within the neighborhood get a highlight border; non-matching neighbors dim further.
- **TRACE**: Search is disabled (trace path is the active filter). Search input is visually inactive.

## Accessibility

Phase 1 baseline:
- `aria-label` on canvas region, context panel, toolbar controls
- `role="tablist"` on granularity dropdown and depth controls
- Esc key exits TRACE → FOCUS → OVERVIEW
- `aria-live="polite"` on context panel to announce state changes ("Focused on verify_signature", "Tracing parameter msg")
- Full keyboard graph navigation (Tab between nodes, arrow key traversal) is deferred to Phase 2

## Testing

Phase 1 test requirements:
- **Hook unit tests**: Each of the 5 hooks tested in isolation (focus computation, trace BFS, depth changes, threshold logic, graph merging)
- **State machine integration tests**: OVERVIEW → FOCUS → TRACE → back transitions with mock graph data
- **Component render tests**: Each custom node type (ClusterNode, FileNode, SymbolNode) renders correctly with fixture data
- Existing `GraphLens.test.tsx` assertions that test graph loading and node rendering are migrated to the new component test file

## What This Does NOT Include

- Findings/vulnerability display (this is a comprehension tool, not a findings dashboard)
- Real API integration (Phase 2)
- Tauri IPC integration (web-first)
- New backend endpoints or project-ir changes
- Collaborative features (Phase 3)

## Success Criteria

Phase 1 is complete when:
1. An auditor can open the web-UI in a browser and see an adaptive overview of the mock codebase
2. Clicking a function node spotlights it with upstream/downstream neighbors visible, rest dimmed
3. The context panel shows the function's typed signature with clickable parameters
4. Clicking a parameter traces its origin backwards through the call chain, highlighted on the graph
5. Depth slider adjusts how many hops of neighbors are shown
6. Granularity dropdown overrides the adaptive view level
7. The interaction feels smooth — no jarring layout shifts, responsive to clicks
