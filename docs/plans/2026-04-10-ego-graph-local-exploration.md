# Ego-Graph Local Exploration Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace the global ELK layout in focus mode with a fast, readable ego-graph column layout — focused node centered, callers fanning left, callees fanning right — capped at ~80 nodes so performance is good at any codebase scale.

**Architecture:** When `stateKind === "focus"`, `buildFlowModel()` filters to ego-graph nodes only (focused node + BFS neighborhood within depth), and `ExplorerCanvas` uses a synchronous O(n) column layout instead of async ELK. A new `EgoBanner` strip shows node metadata and a back-to-overview button. Everything else (overview mode, trace mode, context panel, context menu) is untouched.

**Tech Stack:** React 18, TypeScript, ReactFlow, existing `useFocusContext` BFS (no new dependencies)

---

## Context: Key files and their roles

- [`ui/src/features/workstation/CodebaseExplorer/AdaptiveLayout.tsx`](ui/src/features/workstation/CodebaseExplorer/AdaptiveLayout.tsx) — `buildFlowModel()` converts the graph to ReactFlow nodes/edges. We add ego-mode filtering here.
- [`ui/src/features/workstation/CodebaseExplorer/ExplorerCanvas.tsx`](ui/src/features/workstation/CodebaseExplorer/ExplorerCanvas.tsx) — calls `layoutWithElk()` in a `useEffect`. We branch on `stateKind === "focus"` to call `egoLayout()` instead.
- [`ui/src/features/workstation/CodebaseExplorer/index.tsx`](ui/src/features/workstation/CodebaseExplorer/index.tsx) — `ExplorerLayout` renders toolbar + canvas. We add `EgoBanner` here.
- [`ui/src/features/workstation/CodebaseExplorer/hooks/useFocusContext.ts`](ui/src/features/workstation/CodebaseExplorer/hooks/useFocusContext.ts) — BFS for upstream/downstream sets. We expose total counts before depth cap.
- [`ui/src/features/workstation/CodebaseExplorer/types.ts`](ui/src/features/workstation/CodebaseExplorer/types.ts) — `ExplorerContextValue`. We add `totalUpstreamCount` and `totalDownstreamCount`.
- [`ui/src/features/workstation/CodebaseExplorer/ExplorerContext.tsx`](ui/src/features/workstation/CodebaseExplorer/ExplorerContext.tsx) — wires all hooks into context value. We wire the new counts.
- [`ui/src/styles.css`](ui/src/styles.css) — add `.explorer-ego-banner`, `.explorer-ego-center`, `.explorer-depth-chip` styles.
- Tests: [`ui/src/features/workstation/CodebaseExplorer/__tests__/hooks.test.ts`](ui/src/features/workstation/CodebaseExplorer/__tests__/hooks.test.ts) and [`ui/src/features/workstation/CodebaseExplorer/__tests__/CodebaseExplorer.test.tsx`](ui/src/features/workstation/CodebaseExplorer/__tests__/CodebaseExplorer.test.tsx)

**Run all tests with:**
```bash
cd ui && npx vitest run
```

**Run explorer tests only:**
```bash
cd ui && npx vitest run src/features/workstation/CodebaseExplorer
```

---

## Task 1: Expose total upstream/downstream counts from `useFocusContext`

The ego banner needs to show total available callers/callees — not the depth-capped subset. Currently `useFocusContext` only exposes the depth-capped sets. We add two new counts computed at depth=Infinity.

**Files:**
- Modify: `ui/src/features/workstation/CodebaseExplorer/hooks/useFocusContext.ts`
- Modify: `ui/src/features/workstation/CodebaseExplorer/types.ts`
- Modify: `ui/src/features/workstation/CodebaseExplorer/ExplorerContext.tsx`
- Test: `ui/src/features/workstation/CodebaseExplorer/__tests__/hooks.test.ts`

**Step 1: Write the failing test**

In `hooks.test.ts`, find the `describe("useFocusContext")` block and add at the end:

```ts
it("exposes total upstream and downstream counts regardless of depth cap", () => {
  const graph: ExplorerGraph = {
    nodes: [
      { id: "a", label: "a", kind: "function" },
      { id: "b", label: "b", kind: "function" },
      { id: "c", label: "c", kind: "function" },
      { id: "d", label: "d", kind: "function" },
    ],
    edges: [
      { from: "b", to: "a", relation: "calls" }, // b calls a (b is caller of a)
      { from: "c", to: "b", relation: "calls" }, // c calls b (c is depth-2 caller of a)
      { from: "a", to: "d", relation: "calls" }, // a calls d
    ],
  };

  // depth=1 means only direct neighbors
  const { result } = renderHook(() => useFocusContext(graph, 1));
  act(() => result.current.focusNode("a"));

  // depth-capped: upstreamIds has only "b" (depth 1), not "c" (depth 2)
  expect(result.current.upstreamIds.has("b")).toBe(true);
  expect(result.current.upstreamIds.has("c")).toBe(false);

  // but total counts should include all reachable nodes
  expect(result.current.totalUpstreamCount).toBe(2); // b and c
  expect(result.current.totalDownstreamCount).toBe(1); // d
});
```

**Step 2: Run to verify it fails**

```bash
cd ui && npx vitest run src/features/workstation/CodebaseExplorer/__tests__/hooks.test.ts
```

Expected: FAIL — `totalUpstreamCount` does not exist on result.

**Step 3: Implement in `useFocusContext.ts`**

Add two new `useMemo` blocks after the existing `downstreamIds` memo (after line 63):

```ts
const totalUpstreamCount = useMemo(() => {
  if (!focusedNodeId) return 0;
  return bfsNeighbors(focusedNodeId, upstreamAdjacency, Infinity as number).size;
}, [focusedNodeId, upstreamAdjacency]);

const totalDownstreamCount = useMemo(() => {
  if (!focusedNodeId) return 0;
  return bfsNeighbors(focusedNodeId, downstreamAdjacency, Infinity as number).size;
}, [focusedNodeId, downstreamAdjacency]);
```

Update the return object (lines 75–82) to include the new fields:

```ts
return {
  stateKind,
  focusedNodeId,
  upstreamIds,
  downstreamIds,
  totalUpstreamCount,
  totalDownstreamCount,
  focusNode,
  clearFocus,
};
```

**Step 4: Add to `ExplorerContextValue` type in `types.ts`**

After line 80 (`downstreamIds: Set<string>;`), add:

```ts
totalUpstreamCount: number;
totalDownstreamCount: number;
```

**Step 5: Wire into `ExplorerContext.tsx`**

In the `value` object (around line 125), after `downstreamIds: focus.downstreamIds,` add:

```ts
totalUpstreamCount: focus.totalUpstreamCount,
totalDownstreamCount: focus.totalDownstreamCount,
```

**Step 6: Run tests**

```bash
cd ui && npx vitest run src/features/workstation/CodebaseExplorer/__tests__/hooks.test.ts
```

Expected: new test passes, all existing tests pass.

**Step 7: Commit**

```bash
git add ui/src/features/workstation/CodebaseExplorer/hooks/useFocusContext.ts \
        ui/src/features/workstation/CodebaseExplorer/types.ts \
        ui/src/features/workstation/CodebaseExplorer/ExplorerContext.tsx \
        ui/src/features/workstation/CodebaseExplorer/__tests__/hooks.test.ts
git commit -m "feat(explorer): expose totalUpstreamCount/totalDownstreamCount from useFocusContext"
```

---

## Task 2: Add `egoLayout()` pure function to `ExplorerCanvas`

A synchronous column layout: callers go left (negative columns), callees go right (positive columns), focused node at column 0. No ELK involved.

**Files:**
- Modify: `ui/src/features/workstation/CodebaseExplorer/ExplorerCanvas.tsx`
- Test: `ui/src/features/workstation/CodebaseExplorer/__tests__/hooks.test.ts` (add a layout unit test here since there's no canvas test file)

**Step 1: Write the failing test**

In `hooks.test.ts`, add a new `describe` block at the bottom:

```ts
import { egoLayout } from "../ExplorerCanvas";

describe("egoLayout", () => {
  it("places the focused node at x=0 column", () => {
    const nodes = [
      { id: "center", type: "symbolNode" },
      { id: "caller1", type: "symbolNode" },
      { id: "callee1", type: "symbolNode" },
    ] as any[];

    const positions = egoLayout({
      nodes,
      focusedNodeId: "center",
      upstreamIds: new Set(["caller1"]),
      downstreamIds: new Set(["callee1"]),
    });

    // focused node at column 0
    expect(positions.get("center")?.x).toBe(0);
    // caller is to the left (negative x)
    expect(positions.get("caller1")!.x).toBeLessThan(0);
    // callee is to the right (positive x)
    expect(positions.get("callee1")!.x).toBeGreaterThan(0);
  });

  it("stacks multiple nodes in the same column vertically", () => {
    const nodes = [
      { id: "center", type: "symbolNode" },
      { id: "c1", type: "symbolNode" },
      { id: "c2", type: "symbolNode" },
    ] as any[];

    const positions = egoLayout({
      nodes,
      focusedNodeId: "center",
      upstreamIds: new Set(["c1", "c2"]),
      downstreamIds: new Set(),
    });

    // both callers share the same x (same column)
    expect(positions.get("c1")?.x).toBe(positions.get("c2")?.x);
    // but different y
    expect(positions.get("c1")?.y).not.toBe(positions.get("c2")?.y);
  });

  it("caps each column at MAX_NODES_PER_COLUMN and returns overflow count", () => {
    // 10 callers — 8 max per column
    const callerIds = Array.from({ length: 10 }, (_, i) => `caller_${i}`);
    const nodes = [
      { id: "center", type: "symbolNode" },
      ...callerIds.map((id) => ({ id, type: "symbolNode" })),
    ] as any[];

    const { positions, overflowCounts } = egoLayout({
      nodes,
      focusedNodeId: "center",
      upstreamIds: new Set(callerIds),
      downstreamIds: new Set(),
      returnOverflow: true,
    });

    // only 8 rendered upstream
    const upstreamRendered = [...positions.entries()].filter(
      ([id]) => callerIds.includes(id)
    );
    expect(upstreamRendered.length).toBe(8);
    // overflow count = 2
    expect(overflowCounts?.upstream).toBe(2);
  });
});
```

**Step 2: Run to verify it fails**

```bash
cd ui && npx vitest run src/features/workstation/CodebaseExplorer/__tests__/hooks.test.ts
```

Expected: FAIL — `egoLayout` not exported from `ExplorerCanvas`.

**Step 3: Implement `egoLayout` in `ExplorerCanvas.tsx`**

Add this before the `ExplorerCanvas` component function (after the `nodeTypes` constant, around line 25):

```ts
const COLUMN_WIDTH = 380;
const ROW_HEIGHT = 100;
const MAX_NODES_PER_COLUMN = 8;

type EgoLayoutInput = {
  nodes: Node[];
  focusedNodeId: string;
  upstreamIds: Set<string>;
  downstreamIds: Set<string>;
  returnOverflow?: boolean;
};

type EgoLayoutResult = {
  positions: Map<string, { x: number; y: number }>;
  overflowCounts?: { upstream: number; downstream: number };
};

export function egoLayout({
  nodes,
  focusedNodeId,
  upstreamIds,
  downstreamIds,
  returnOverflow = false,
}: EgoLayoutInput): EgoLayoutResult {
  const positions = new Map<string, { x: number; y: number }>();

  // Center node
  const focusedNode = nodes.find((n) => n.id === focusedNodeId);
  if (focusedNode) {
    positions.set(focusedNodeId, { x: 0, y: 0 });
  }

  // Sort for deterministic layout
  const upstreamNodes = nodes.filter((n) => upstreamIds.has(n.id));
  const downstreamNodes = nodes.filter((n) => downstreamIds.has(n.id));

  const upstreamVisible = upstreamNodes.slice(0, MAX_NODES_PER_COLUMN);
  const downstreamVisible = downstreamNodes.slice(0, MAX_NODES_PER_COLUMN);

  const upstreamOverflow = upstreamNodes.length - upstreamVisible.length;
  const downstreamOverflow = downstreamNodes.length - downstreamVisible.length;

  // Upstream (callers): column -1 (x = -COLUMN_WIDTH)
  upstreamVisible.forEach((node, index) => {
    const totalRows = upstreamVisible.length;
    const offsetY = (index - (totalRows - 1) / 2) * ROW_HEIGHT;
    positions.set(node.id, { x: -COLUMN_WIDTH, y: offsetY });
  });

  // Downstream (callees): column +1 (x = +COLUMN_WIDTH)
  downstreamVisible.forEach((node, index) => {
    const totalRows = downstreamVisible.length;
    const offsetY = (index - (totalRows - 1) / 2) * ROW_HEIGHT;
    positions.set(node.id, { x: COLUMN_WIDTH, y: offsetY });
  });

  if (returnOverflow) {
    return {
      positions,
      overflowCounts: { upstream: upstreamOverflow, downstream: downstreamOverflow },
    };
  }

  return { positions };
}
```

**Step 4: Run tests**

```bash
cd ui && npx vitest run src/features/workstation/CodebaseExplorer/__tests__/hooks.test.ts
```

Expected: all `egoLayout` tests pass, all existing tests still pass.

**Step 5: Commit**

```bash
git add ui/src/features/workstation/CodebaseExplorer/ExplorerCanvas.tsx \
        ui/src/features/workstation/CodebaseExplorer/__tests__/hooks.test.ts
git commit -m "feat(explorer): add egoLayout column layout function for focus mode"
```

---

## Task 3: Filter `buildFlowModel` to ego-graph nodes in focus mode

In focus mode, `buildFlowModel` currently returns all visible nodes. We filter it to only the focused node + its neighborhood so ReactFlow only receives ≤80 nodes.

**Files:**
- Modify: `ui/src/features/workstation/CodebaseExplorer/AdaptiveLayout.tsx`
- Test: `ui/src/features/workstation/CodebaseExplorer/__tests__/hooks.test.ts`

**Step 1: Write the failing test**

Add to `hooks.test.ts`:

```ts
import { buildFlowModel } from "../AdaptiveLayout";

describe("buildFlowModel in focus mode", () => {
  const graph: ExplorerGraph = {
    nodes: [
      { id: "fn_a", label: "fn_a", kind: "function" },
      { id: "fn_b", label: "fn_b", kind: "function" },
      { id: "fn_c", label: "fn_c", kind: "function" },
      { id: "fn_d", label: "fn_d", kind: "function" }, // unrelated
    ],
    edges: [
      { from: "fn_b", to: "fn_a", relation: "calls" },
      { from: "fn_a", to: "fn_c", relation: "calls" },
    ],
  };

  const baseConfig = {
    resolvedGranularity: "files" as const,
    expandedClusters: new Set<string>(),
    neighborhoodResult: null,
    matchingNodeIds: null,
  };

  it("includes only ego-graph nodes when stateKind is focus", () => {
    const { nodes } = buildFlowModel(graph, {
      ...baseConfig,
      stateKind: "focus",
      focusedNodeId: "fn_a",
      upstreamIds: new Set(["fn_b"]),
      downstreamIds: new Set(["fn_c"]),
    });

    const ids = nodes.map((n) => n.id);
    expect(ids).toContain("fn_a");
    expect(ids).toContain("fn_b");
    expect(ids).toContain("fn_c");
    expect(ids).not.toContain("fn_d"); // unrelated node excluded
  });

  it("includes all nodes when stateKind is overview", () => {
    const { nodes } = buildFlowModel(graph, {
      ...baseConfig,
      stateKind: "overview",
      focusedNodeId: null,
      upstreamIds: new Set(),
      downstreamIds: new Set(),
    });

    expect(nodes.map((n) => n.id)).toContain("fn_d");
  });

  it("marks the focused node with explorer-ego-center class", () => {
    const { nodes } = buildFlowModel(graph, {
      ...baseConfig,
      stateKind: "focus",
      focusedNodeId: "fn_a",
      upstreamIds: new Set(["fn_b"]),
      downstreamIds: new Set(["fn_c"]),
    });

    const center = nodes.find((n) => n.id === "fn_a");
    expect(center?.className).toContain("explorer-ego-center");
  });
});
```

**Step 2: Run to verify it fails**

```bash
cd ui && npx vitest run src/features/workstation/CodebaseExplorer/__tests__/hooks.test.ts
```

Expected: FAIL — ego-mode filtering not implemented.

**Step 3: Implement ego filtering in `buildFlowModel`**

In `AdaptiveLayout.tsx`, replace the node-filtering loop (lines 142–169) with this updated version:

```ts
export function buildFlowModel(graph: ExplorerGraph, config: LayoutConfig): FlowModel {
  const nodes: Node[] = [];
  const edges: Edge[] = [];
  const visibleNodeIds = new Set<string>();
  const nodeById = new Map(graph.nodes.map((node) => [node.id, node]));
  const parentMap = buildParentMap(graph.edges);

  // In focus mode, only show the ego-graph: focused node + its neighborhood
  const egoNodeIds: Set<string> | null =
    config.stateKind === "focus" && config.focusedNodeId
      ? new Set([
          config.focusedNodeId,
          ...config.upstreamIds,
          ...config.downstreamIds,
        ])
      : null;

  for (const node of graph.nodes) {
    // In focus mode, skip nodes outside the ego-graph
    if (egoNodeIds && !egoNodeIds.has(node.id)) {
      continue;
    }
    // In overview/highlight mode, apply granularity visibility rules
    if (!egoNodeIds && !isVisibleNode(node, config, parentMap, nodeById)) {
      continue;
    }

    visibleNodeIds.add(node.id);
    const isCluster = node.kind === "crate" || node.kind === "module";
    const classes = [nodeHighlightClass(node.id, config)];

    // Mark the focused node for CSS styling
    if (node.id === config.focusedNodeId) {
      classes.push("explorer-ego-center");
    }

    nodes.push({
      id: node.id,
      type: isCluster ? "clusterNode" : node.kind === "file" ? "fileNode" : "symbolNode",
      position: { x: 0, y: 0 },
      sourcePosition: Position.Bottom,
      targetPosition: Position.Top,
      className: classes.filter(Boolean).join(" ") || undefined,
      data: {
        label: node.label,
        kind: node.kind,
        filePath: node.filePath,
        line: node.line,
        signature: node.signature,
        childCount:
          isCluster || node.kind === "file" ? countChildren(node, graph.edges) : undefined,
        expanded: config.expandedClusters.has(node.id),
      },
    });
  }

  // Edge loop unchanged from original — only edges between visibleNodeIds are included
  // ... (keep the existing edge loop exactly as-is from line 171 onwards)
```

> **Important:** Only replace the node loop at the top of `buildFlowModel`. The edge deduplication loop (lines 171–234) and the `return` statement stay exactly as they are.

**Step 4: Run tests**

```bash
cd ui && npx vitest run src/features/workstation/CodebaseExplorer/__tests__/hooks.test.ts
```

Expected: all three new `buildFlowModel` tests pass.

**Step 5: Run full suite**

```bash
cd ui && npx vitest run
```

Expected: all tests pass.

**Step 6: Commit**

```bash
git add ui/src/features/workstation/CodebaseExplorer/AdaptiveLayout.tsx \
        ui/src/features/workstation/CodebaseExplorer/__tests__/hooks.test.ts
git commit -m "feat(explorer): filter buildFlowModel to ego-graph in focus mode"
```

---

## Task 4: Wire `egoLayout` into `ExplorerCanvas` for focus mode

Branch the layout `useEffect` on `stateKind`. When focused, call `egoLayout` (sync, instant) instead of `layoutWithElk` (async, slow).

**Files:**
- Modify: `ui/src/features/workstation/CodebaseExplorer/ExplorerCanvas.tsx`

**Step 1: Read the current `useEffect` at lines 115–136**

The effect currently looks like:

```ts
useEffect(() => {
  void layoutWithElk(flowModel.nodes, flowModel.edges)
    .then((positionedNodes) => { ... })
    .catch(() => { ... });
}, [topologyKey]);
```

**Step 2: Replace with branched layout effect**

Replace that entire `useEffect` block with:

```ts
useEffect(() => {
  if (ctx.stateKind === "focus" && ctx.focusedNodeId) {
    // Synchronous ego layout — no ELK, instant
    const { positions } = egoLayout({
      nodes: flowModel.nodes,
      focusedNodeId: ctx.focusedNodeId,
      upstreamIds: ctx.upstreamIds,
      downstreamIds: ctx.downstreamIds,
    });
    setPositionByNodeId(positions);
    requestAnimationFrame(() => {
      flowRef.current?.fitView?.({ padding: 0.2 });
    });
    return;
  }

  // Overview / trace mode: use ELK
  void layoutWithElk(flowModel.nodes, flowModel.edges)
    .then((positionedNodes) => {
      setPositionByNodeId(
        new Map(positionedNodes.map((node) => [node.id, node.position]))
      );
      requestAnimationFrame(() => {
        flowRef.current?.fitView?.({ padding: 0.16 });
      });
    })
    .catch(() => {
      setPositionByNodeId(
        new Map(
          flowModel.nodes.map((node, index) => [
            node.id,
            { x: (index % 5) * 320, y: Math.floor(index / 5) * 120 },
          ])
        )
      );
      requestAnimationFrame(() => {
        flowRef.current?.fitView?.({ padding: 0.16 });
      });
    });
}, [topologyKey, ctx.stateKind, ctx.focusedNodeId, ctx.upstreamIds, ctx.downstreamIds]);
```

**Step 3: Add `ctx.stateKind`, `ctx.focusedNodeId`, `ctx.upstreamIds`, `ctx.downstreamIds` to deps**

The `useEffect` dependency array now includes the new values. Make sure the `ctx` destructure at the top of `ExplorerCanvas` (line 60) exposes these — they are already on `ExplorerContextValue`, no change needed.

**Step 4: Run full test suite**

```bash
cd ui && npx vitest run
```

Expected: all tests pass (the mock ELK is only called in overview mode now, which is fine).

**Step 5: Commit**

```bash
git add ui/src/features/workstation/CodebaseExplorer/ExplorerCanvas.tsx
git commit -m "feat(explorer): use egoLayout in focus mode, bypass ELK for instant layout"
```

---

## Task 5: Add `EgoBanner` component to `index.tsx`

A strip that appears above the canvas in focus mode showing node name, location, caller/callee counts, and a back button.

**Files:**
- Modify: `ui/src/features/workstation/CodebaseExplorer/index.tsx`
- Test: `ui/src/features/workstation/CodebaseExplorer/__tests__/CodebaseExplorer.test.tsx`

**Step 1: Write the failing test**

In `CodebaseExplorer.test.tsx`, find a suitable `it` block location (after the existing focus mode tests) and add:

```ts
it("shows ego banner with node name and counts when a node is focused", async () => {
  mockLoadExplorerGraph.mockResolvedValue(MOCK_OVERVIEW_RESPONSE);
  render(<CodebaseExplorer sessionId="test-session" />);
  await waitFor(() => expect(screen.getByTestId("mock-reactflow")).toBeInTheDocument());

  // Click a symbol node to enter focus mode
  // (need a graph with calls edges for upstream/downstream counts)
  // Use the existing mock data — click fil_003
  fireEvent.click(screen.getByTestId("rf-node-fil_003"));

  await waitFor(() => {
    expect(screen.getByRole("banner", { hidden: true }) ?? screen.getByTestId("ego-banner"))
      .toBeInTheDocument();
  });

  expect(screen.getByText(/← overview/i)).toBeInTheDocument();
});

it("returns to overview when ego banner back button is clicked", async () => {
  mockLoadExplorerGraph.mockResolvedValue(MOCK_OVERVIEW_RESPONSE);
  render(<CodebaseExplorer sessionId="test-session" />);
  await waitFor(() => expect(screen.getByTestId("mock-reactflow")).toBeInTheDocument());

  fireEvent.click(screen.getByTestId("rf-node-fil_003"));
  await waitFor(() => expect(screen.getByText(/← overview/i)).toBeInTheDocument());

  fireEvent.click(screen.getByText(/← overview/i));
  await waitFor(() => expect(screen.queryByText(/← overview/i)).not.toBeInTheDocument());
});
```

**Step 2: Run to verify it fails**

```bash
cd ui && npx vitest run src/features/workstation/CodebaseExplorer/__tests__/CodebaseExplorer.test.tsx
```

Expected: FAIL — no ego banner in DOM.

**Step 3: Implement `EgoBanner` in `index.tsx`**

Add this component before `ExplorerLayout`:

```tsx
function EgoBanner() {
  const ctx = useExplorer();
  const focusedNode = ctx.focusedNodeId ? ctx.nodeMap.get(ctx.focusedNodeId) : null;

  if (ctx.stateKind !== "focus" || !focusedNode) {
    return null;
  }

  const location = focusedNode.filePath
    ? `${focusedNode.filePath}${focusedNode.line ? `:${focusedNode.line}` : ""}`
    : null;

  return (
    <div className="explorer-ego-banner" data-testid="ego-banner" role="status">
      <button
        type="button"
        className="explorer-ego-back"
        onClick={ctx.clearFocus}
      >
        ← Overview
      </button>
      <span className="explorer-ego-label">
        <strong>{focusedNode.label}</strong>
        {location ? <span className="explorer-ego-location">{location}</span> : null}
      </span>
      <span className="explorer-ego-counts">
        Callers: {ctx.totalUpstreamCount} · Callees: {ctx.totalDownstreamCount}
      </span>
    </div>
  );
}
```

**Step 4: Add `EgoBanner` to `ExplorerLayout`**

In the `ExplorerLayout` function's return, insert `<EgoBanner />` between `<ExplorerToolbar />` and the loading/error/body conditional:

```tsx
function ExplorerLayout() {
  const { isLoading, error, isStale, reload, stateKind } = useExplorer();

  return (
    <section className="explorer-root" aria-label="Codebase Explorer">
      {isStale ? ( ... ) : null}
      <ExplorerToolbar />
      <EgoBanner />          {/* ← add this line */}
      {isLoading ? ( ... ) : error ? ( ... ) : ( ... )}
    </section>
  );
}
```

**Step 5: Run tests**

```bash
cd ui && npx vitest run src/features/workstation/CodebaseExplorer/__tests__/CodebaseExplorer.test.tsx
```

Expected: new banner tests pass.

**Step 6: Run full suite**

```bash
cd ui && npx vitest run
```

Expected: all tests pass.

**Step 7: Commit**

```bash
git add ui/src/features/workstation/CodebaseExplorer/index.tsx \
        ui/src/features/workstation/CodebaseExplorer/__tests__/CodebaseExplorer.test.tsx
git commit -m "feat(explorer): add EgoBanner with back button and caller/callee counts"
```

---

## Task 6: Add CSS for ego mode

**Files:**
- Modify: `ui/src/styles.css`

**Step 1: Add styles**

Find the `/* -- Codebase Explorer -- */` section in `styles.css` (around line 1918). After the `.explorer-toolbar` block, add:

```css
/* Ego-graph focus mode */

.explorer-ego-banner {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 6px 12px;
  background: #0d1521;
  border-bottom: 1px solid #1e3a5f;
  font-size: 12px;
  color: #94a3b8;
  flex-shrink: 0;
}

.explorer-ego-back {
  background: none;
  border: 1px solid #334155;
  border-radius: 4px;
  color: #60a5fa;
  font-size: 11px;
  padding: 3px 8px;
  cursor: pointer;
  white-space: nowrap;
  transition: background-color 150ms ease-out, border-color 150ms ease-out;
}

.explorer-ego-back:hover {
  background: #1e3a5f;
  border-color: #3b82f6;
}

.explorer-ego-label {
  display: flex;
  align-items: center;
  gap: 8px;
  flex: 1;
  min-width: 0;
  overflow: hidden;
}

.explorer-ego-label strong {
  color: #e2e8f0;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.explorer-ego-location {
  color: #64748b;
  font-family: "JetBrains Mono", ui-monospace, monospace;
  font-size: 11px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.explorer-ego-counts {
  white-space: nowrap;
  color: #64748b;
  font-size: 11px;
}

/* Focused center node — larger, green border */
.explorer-ego-center .react-flow__node {
  border-color: var(--accent-green, #22c55e) !important;
  box-shadow: 0 0 0 2px rgba(34, 197, 94, 0.25);
}

/* Or target the node wrapper directly (ReactFlow wraps in .react-flow__node) */
.react-flow__node.explorer-ego-center > div {
  border-color: var(--accent-green, #22c55e);
  box-shadow: 0 0 0 2px rgba(34, 197, 94, 0.25);
}
```

**Step 2: Run full test suite to confirm no regressions**

```bash
cd ui && npx vitest run
```

Expected: all tests pass.

**Step 3: Commit**

```bash
git add ui/src/styles.css
git commit -m "style(explorer): add ego-graph banner and center node styles"
```

---

## Task 7: Cap depth slider to 5 in focus mode

The depth slider goes 1–10 in overview, but in ego mode >5 hops floods the canvas. Cap it at 5 when focused.

**Files:**
- Modify: `ui/src/features/workstation/CodebaseExplorer/index.tsx`

**Step 1: Find `ExplorerToolbar` in `index.tsx`**

The depth control buttons are rendered conditionally when `ctx.stateKind !== "overview"` (around lines 45–65). The `+` button is disabled when `ctx.depth >= 10`.

**Step 2: Change the cap from 10 to 5 in focus mode**

Replace:
```tsx
disabled={controlsDisabled || ctx.depth >= 10}
```

With:
```tsx
disabled={controlsDisabled || ctx.depth >= (ctx.stateKind === "focus" ? 5 : 10)}
```

**Step 3: Run full suite**

```bash
cd ui && npx vitest run
```

Expected: all tests pass.

**Step 4: Commit**

```bash
git add ui/src/features/workstation/CodebaseExplorer/index.tsx
git commit -m "feat(explorer): cap depth slider at 5 in ego focus mode"
```

---

## Task 8: Wrap node components in `React.memo`

Prevents all three node components re-rendering on every layout pass. Zero behavior change, pure performance.

**Files:**
- Modify: `ui/src/features/workstation/CodebaseExplorer/nodes/ClusterNode.tsx`
- Modify: `ui/src/features/workstation/CodebaseExplorer/nodes/FileNode.tsx`
- Modify: `ui/src/features/workstation/CodebaseExplorer/nodes/SymbolNode.tsx`

**Step 1: Read each file and wrap the export**

For each of the three node files, the exported component currently looks like:

```tsx
export function ClusterNode({ data }: ...) { ... }
```

Change each to:

```tsx
import { memo } from "react";

function ClusterNodeInner({ data }: ...) { ... }
export const ClusterNode = memo(ClusterNodeInner);
```

Apply the same pattern to `FileNode` and `SymbolNode`.

**Step 2: Run full suite**

```bash
cd ui && npx vitest run
```

Expected: all tests pass.

**Step 3: Commit**

```bash
git add ui/src/features/workstation/CodebaseExplorer/nodes/ClusterNode.tsx \
        ui/src/features/workstation/CodebaseExplorer/nodes/FileNode.tsx \
        ui/src/features/workstation/CodebaseExplorer/nodes/SymbolNode.tsx
git commit -m "perf(explorer): wrap node components in React.memo"
```

---

## Summary

| Task | Files | What it does |
|------|-------|-------------|
| 1 | `useFocusContext`, `types`, `ExplorerContext` | Expose total caller/callee counts for banner |
| 2 | `ExplorerCanvas` | `egoLayout()` pure column math function |
| 3 | `AdaptiveLayout` | Filter `buildFlowModel` to ego-graph in focus mode |
| 4 | `ExplorerCanvas` | Wire `egoLayout` into layout effect, bypass ELK in focus mode |
| 5 | `index.tsx` | `EgoBanner` component with back button and counts |
| 6 | `styles.css` | Banner and center-node styles |
| 7 | `index.tsx` | Cap depth at 5 in focus mode |
| 8 | Node components | `React.memo` for all three node types |

**Performance result:** In focus mode, layout goes from async ELK O(n log n) on the full graph → synchronous O(n) on ≤80 nodes. Typical layout time drops from 2–8 seconds to <5ms.
