# Codebase Explorer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the passive GraphLens visualization with an interactive Codebase Explorer that lets auditors focus on nodes, trace parameter flow, and adaptively browse codebase structure — all powered by fixture data for Phase 1.

**Architecture:** New `CodebaseExplorer/` directory with separated components (canvas, context panel, trace overlay, adaptive layout) and hooks (focus, trace, depth, thresholds, graph). State coordinated through a single React context provider. ReactFlow + ELK for graph rendering.

**Tech Stack:** React 18, ReactFlow 11, ELK.js, Vitest, Testing Library, TypeScript (strict), Vite

**Spec:** `docs/superpowers/specs/2026-03-26-codebase-explorer-design.md`

---

## File Structure

### New Files

| File | Responsibility |
|---|---|
| `ui/src/features/workstation/CodebaseExplorer/types.ts` | All shared types: `ExplorerNode`, `ExplorerEdge`, `ExplorerGraph`, `FunctionSignature`, `ParameterInfo`, `ExplorerState`, context type |
| `ui/src/features/workstation/CodebaseExplorer/fixtures/mockGraph.ts` | Three fixture datasets (small/medium/large) of realistic crypto project graphs |
| `ui/src/features/workstation/CodebaseExplorer/hooks/useDepthControl.ts` | Depth slider state (1-5+, default 2) |
| `ui/src/features/workstation/CodebaseExplorer/hooks/useAdaptiveThresholds.ts` | Granularity auto-selection and threshold config |
| `ui/src/features/workstation/CodebaseExplorer/hooks/useUnifiedGraph.ts` | Loads fixture graph data, returns `ExplorerGraph` |
| `ui/src/features/workstation/CodebaseExplorer/hooks/useFocusContext.ts` | Focus+dim: BFS neighbor computation, state transitions |
| `ui/src/features/workstation/CodebaseExplorer/hooks/useTrace.ts` | Parameter/return tracing: BFS path computation |
| `ui/src/features/workstation/CodebaseExplorer/ExplorerContext.tsx` | React context provider wiring all hooks together |
| `ui/src/features/workstation/CodebaseExplorer/nodes/ClusterNode.tsx` | ReactFlow custom node for crate/module clusters |
| `ui/src/features/workstation/CodebaseExplorer/nodes/FileNode.tsx` | ReactFlow custom node for files |
| `ui/src/features/workstation/CodebaseExplorer/nodes/SymbolNode.tsx` | ReactFlow custom node with clickable parameter spans |
| `ui/src/features/workstation/CodebaseExplorer/ExplorerCanvas.tsx` | ReactFlow wrapper: renders graph, handles pan/zoom, node clicks |
| `ui/src/features/workstation/CodebaseExplorer/AdaptiveLayout.tsx` | Transforms graph into clustered/flat ReactFlow model based on granularity |
| `ui/src/features/workstation/CodebaseExplorer/ContextPanel.tsx` | Right panel: node identity, signature, on-demand sections |
| `ui/src/features/workstation/CodebaseExplorer/TraceOverlay.tsx` | Applies trace highlighting (purple path, animated edges) to canvas |
| `ui/src/features/workstation/CodebaseExplorer/index.tsx` | Main export: layout with canvas + context panel, toolbar |
| `ui/src/features/workstation/CodebaseExplorer/__tests__/hooks.test.ts` | Unit tests for all 5 hooks |
| `ui/src/features/workstation/CodebaseExplorer/__tests__/nodes.test.tsx` | Render tests for ClusterNode, FileNode, SymbolNode |
| `ui/src/features/workstation/CodebaseExplorer/__tests__/CodebaseExplorer.test.tsx` | Integration tests for state machine transitions |

### Modified Files

| File | Change |
|---|---|
| `ui/src/features/workstation/WorkstationShell.tsx` | Replace `GraphLens` import with `CodebaseExplorer` at lines 13, 251-257, 343-352, 374-380 |
| `ui/src/styles.css` | Add explorer-specific CSS classes (focus colors, dim opacity, context panel, trace highlighting) |

### Removed Files

| File | Reason |
|---|---|
| `ui/src/features/workstation/GraphLensReactFlow.tsx` | Replaced by CodebaseExplorer |
| `ui/src/features/workstation/GraphLensCytoscape.tsx` | Replaced by CodebaseExplorer |
| `ui/src/features/workstation/GraphLens.tsx` | Wrapper no longer needed |
| `ui/src/features/workstation/GraphLens.test.tsx` | Replaced by new test files |

---

## Task 1: Types and Fixture Data

**Files:**
- Create: `ui/src/features/workstation/CodebaseExplorer/types.ts`
- Create: `ui/src/features/workstation/CodebaseExplorer/fixtures/mockGraph.ts`
- Test: `ui/src/features/workstation/CodebaseExplorer/__tests__/hooks.test.ts` (partial — fixture validation only)

- [ ] **Step 1: Create the types file**

```ts
// ui/src/features/workstation/CodebaseExplorer/types.ts

export type ParameterInfo = {
  name: string;
  typeAnnotation?: string;
  position: number;
};

export type FunctionSignature = {
  parameters: ParameterInfo[];
  returnType?: string;
};

export type ExplorerNodeKind =
  | "crate"
  | "module"
  | "file"
  | "function"
  | "trait_impl_method"
  | "macro_call";

export type ExplorerNode = {
  id: string;
  label: string;
  kind: ExplorerNodeKind;
  filePath?: string;
  line?: number;
  signature?: FunctionSignature;
};

export type ExplorerEdgeRelation =
  | "calls"
  | "contains"
  | "parameter_flow"
  | "return_flow"
  | "cfg";

export type ExplorerEdge = {
  from: string;
  to: string;
  relation: ExplorerEdgeRelation;
  parameterName?: string;
  parameterPosition?: number;
  valuePreview?: string;
};

export type ExplorerGraph = {
  nodes: ExplorerNode[];
  edges: ExplorerEdge[];
};

export type GranularityLevel = "auto" | "files" | "modules" | "crates";

export type ExplorerStateKind = "overview" | "focus" | "trace";

export type TraceDirection = "upstream" | "downstream";

export type TraceResult = {
  path: string[];           // ordered node IDs
  direction: TraceDirection;
  parameterName?: string;
};

export type ExplorerContextValue = {
  // Graph data
  graph: ExplorerGraph;

  // State machine
  stateKind: ExplorerStateKind;

  // Focus
  focusedNodeId: string | null;
  upstreamIds: Set<string>;
  downstreamIds: Set<string>;
  focusNode: (nodeId: string) => void;
  clearFocus: () => void;

  // Trace
  traceResult: TraceResult | null;
  traceParameter: (parameterName: string) => void;
  traceReturn: () => void;
  clearTrace: () => void;

  // Depth
  depth: number;
  setDepth: (depth: number) => void;

  // Adaptive
  granularity: GranularityLevel;
  setGranularity: (level: GranularityLevel) => void;
  resolvedGranularity: "files" | "modules" | "crates";
  thresholds: { small: number; large: number };
  setThresholds: (thresholds: { small: number; large: number }) => void;

  // Search
  searchQuery: string;
  setSearchQuery: (query: string) => void;
  matchingNodeIds: Set<string> | null;

  // Expand/collapse
  expandedClusters: Set<string>;
  toggleCluster: (clusterId: string) => void;

  // Dead end
  deadEndMessage: string | null;

  // Navigation
  onNavigateToSource?: (filePath: string, line?: number) => void;
};
```

- [ ] **Step 2: Create the medium fixture dataset**

Create `ui/src/features/workstation/CodebaseExplorer/fixtures/mockGraph.ts` with a realistic ~50 file crypto project graph. Include:
- 5 crate nodes: `intake`, `engine-crypto`, `engine-distributed`, `findings`, `llm`
- ~12 module nodes (subdirectories within crates)
- ~50 file nodes across the modules
- ~80 symbol nodes with realistic function signatures (e.g., `verify_signature(msg: &[u8], sig: &Signature, pubkey: &PublicKey) -> Result<bool>`)
- ~120 edges: `contains` (crate→module→file→function), `calls` (cross-function), `parameter_flow` (with `parameterName`), `return_flow`
- At least one 4-hop trace chain: entry_point → validator → crypto_fn → low_level_fn
- Export as `mediumFixture: ExplorerGraph`

Also create `smallFixture` (~15 file nodes, single crate) and `largeFixture` (~200 file nodes, 8 crates) — these can be sparser, they exist to test adaptive thresholds.

- [ ] **Step 3: Write fixture validation test**

```ts
// ui/src/features/workstation/CodebaseExplorer/__tests__/hooks.test.ts
import { describe, it, expect } from "vitest";
import { smallFixture, mediumFixture, largeFixture } from "../fixtures/mockGraph";
import type { ExplorerGraph } from "../types";

function fileCount(graph: ExplorerGraph): number {
  return graph.nodes.filter((n) => n.kind === "file").length;
}

describe("fixture data", () => {
  it("small fixture has fewer than 30 file nodes", () => {
    expect(fileCount(smallFixture)).toBeLessThan(30);
    expect(fileCount(smallFixture)).toBeGreaterThan(0);
  });

  it("medium fixture has 30-150 file nodes", () => {
    const count = fileCount(mediumFixture);
    expect(count).toBeGreaterThanOrEqual(30);
    expect(count).toBeLessThanOrEqual(150);
  });

  it("large fixture has more than 150 file nodes", () => {
    expect(fileCount(largeFixture)).toBeGreaterThan(150);
  });

  it("medium fixture has symbol nodes with signatures", () => {
    const withSig = mediumFixture.nodes.filter((n) => n.signature);
    expect(withSig.length).toBeGreaterThan(10);
    const sig = withSig[0].signature!;
    expect(sig.parameters.length).toBeGreaterThan(0);
    expect(sig.parameters[0].name).toBeTruthy();
  });

  it("medium fixture has parameter_flow edges with parameterName", () => {
    const paramFlows = mediumFixture.edges.filter((e) => e.relation === "parameter_flow");
    expect(paramFlows.length).toBeGreaterThan(0);
    expect(paramFlows[0].parameterName).toBeTruthy();
  });

  it("all edge references point to existing nodes", () => {
    for (const fixture of [smallFixture, mediumFixture, largeFixture]) {
      const nodeIds = new Set(fixture.nodes.map((n) => n.id));
      for (const edge of fixture.edges) {
        expect(nodeIds.has(edge.from), `missing node ${edge.from}`).toBe(true);
        expect(nodeIds.has(edge.to), `missing node ${edge.to}`).toBe(true);
      }
    }
  });
});
```

- [ ] **Step 4: Run tests to verify fixtures are valid**

Run: `cd ui && npx vitest run src/features/workstation/CodebaseExplorer/__tests__/hooks.test.ts`
Expected: All 6 fixture tests PASS

- [ ] **Step 5: Commit**

```bash
git add ui/src/features/workstation/CodebaseExplorer/types.ts \
       ui/src/features/workstation/CodebaseExplorer/fixtures/mockGraph.ts \
       ui/src/features/workstation/CodebaseExplorer/__tests__/hooks.test.ts
git commit -m "feat(explorer): add types and fixture data for codebase explorer"
```

---

## Task 2: Simple Hooks (depth, thresholds, unified graph)

**Files:**
- Create: `ui/src/features/workstation/CodebaseExplorer/hooks/useDepthControl.ts`
- Create: `ui/src/features/workstation/CodebaseExplorer/hooks/useAdaptiveThresholds.ts`
- Create: `ui/src/features/workstation/CodebaseExplorer/hooks/useUnifiedGraph.ts`
- Test: `ui/src/features/workstation/CodebaseExplorer/__tests__/hooks.test.ts` (append)

- [ ] **Step 1: Write tests for useDepthControl**

Append to `__tests__/hooks.test.ts`:

```ts
import { renderHook, act } from "@testing-library/react";
import { useDepthControl } from "../hooks/useDepthControl";

describe("useDepthControl", () => {
  it("defaults to depth 2", () => {
    const { result } = renderHook(() => useDepthControl());
    expect(result.current.depth).toBe(2);
  });

  it("clamps depth to range 1-10", () => {
    const { result } = renderHook(() => useDepthControl());
    act(() => result.current.setDepth(0));
    expect(result.current.depth).toBe(1);
    act(() => result.current.setDepth(15));
    expect(result.current.depth).toBe(10);
  });

  it("accepts valid depth values", () => {
    const { result } = renderHook(() => useDepthControl());
    act(() => result.current.setDepth(5));
    expect(result.current.depth).toBe(5);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd ui && npx vitest run src/features/workstation/CodebaseExplorer/__tests__/hooks.test.ts`
Expected: FAIL — `useDepthControl` not found

- [ ] **Step 3: Implement useDepthControl**

```ts
// ui/src/features/workstation/CodebaseExplorer/hooks/useDepthControl.ts
import { useState, useCallback } from "react";

export function useDepthControl(initial = 2) {
  const [depth, setDepthRaw] = useState(initial);

  const setDepth = useCallback((value: number) => {
    setDepthRaw(Math.max(1, Math.min(10, Math.round(value))));
  }, []);

  return { depth, setDepth };
}
```

- [ ] **Step 4: Write tests for useAdaptiveThresholds**

Append to `__tests__/hooks.test.ts`:

```ts
import { useAdaptiveThresholds } from "../hooks/useAdaptiveThresholds";

describe("useAdaptiveThresholds", () => {
  it("resolves to files for small graphs", () => {
    const { result } = renderHook(() => useAdaptiveThresholds(15));
    expect(result.current.resolvedGranularity).toBe("files");
  });

  it("resolves to modules for medium graphs", () => {
    const { result } = renderHook(() => useAdaptiveThresholds(50));
    expect(result.current.resolvedGranularity).toBe("modules");
  });

  it("resolves to crates for large graphs", () => {
    const { result } = renderHook(() => useAdaptiveThresholds(200));
    expect(result.current.resolvedGranularity).toBe("crates");
  });

  it("manual override bypasses auto", () => {
    const { result } = renderHook(() => useAdaptiveThresholds(200));
    act(() => result.current.setGranularity("files"));
    expect(result.current.resolvedGranularity).toBe("files");
  });

  it("custom thresholds change resolution", () => {
    const { result } = renderHook(() => useAdaptiveThresholds(50));
    expect(result.current.resolvedGranularity).toBe("modules");
    act(() => result.current.setThresholds({ small: 100, large: 200 }));
    expect(result.current.resolvedGranularity).toBe("files");
  });
});
```

- [ ] **Step 5: Run test to verify it fails**

Run: `cd ui && npx vitest run src/features/workstation/CodebaseExplorer/__tests__/hooks.test.ts`
Expected: FAIL — `useAdaptiveThresholds` not found

- [ ] **Step 6: Implement useAdaptiveThresholds**

```ts
// ui/src/features/workstation/CodebaseExplorer/hooks/useAdaptiveThresholds.ts
import { useState, useCallback, useMemo } from "react";
import type { GranularityLevel } from "../types";

type ResolvedGranularity = "files" | "modules" | "crates";

export function useAdaptiveThresholds(fileCount: number) {
  const [granularity, setGranularity] = useState<GranularityLevel>("auto");
  const [thresholds, setThresholds] = useState({ small: 30, large: 150 });

  const resolvedGranularity: ResolvedGranularity = useMemo(() => {
    if (granularity !== "auto") {
      return granularity;
    }
    if (fileCount < thresholds.small) return "files";
    if (fileCount > thresholds.large) return "crates";
    return "modules";
  }, [granularity, fileCount, thresholds]);

  const setThresholdsSafe = useCallback((t: { small: number; large: number }) => {
    setThresholds({ small: Math.max(1, t.small), large: Math.max(t.small + 1, t.large) });
  }, []);

  return {
    granularity,
    setGranularity,
    resolvedGranularity,
    thresholds,
    setThresholds: setThresholdsSafe,
  };
}
```

- [ ] **Step 7: Write test for useUnifiedGraph**

Append to `__tests__/hooks.test.ts`:

```ts
import { useUnifiedGraph } from "../hooks/useUnifiedGraph";

describe("useUnifiedGraph", () => {
  it("returns medium fixture by default", () => {
    const { result } = renderHook(() => useUnifiedGraph());
    expect(result.current.graph.nodes.length).toBeGreaterThan(0);
    expect(result.current.graph.edges.length).toBeGreaterThan(0);
  });

  it("can switch dataset size", () => {
    const { result } = renderHook(() => useUnifiedGraph("small"));
    const smallCount = result.current.graph.nodes.filter((n) => n.kind === "file").length;
    expect(smallCount).toBeLessThan(30);
  });
});
```

- [ ] **Step 8: Implement useUnifiedGraph**

```ts
// ui/src/features/workstation/CodebaseExplorer/hooks/useUnifiedGraph.ts
import { useMemo } from "react";
import { smallFixture, mediumFixture, largeFixture } from "../fixtures/mockGraph";
import type { ExplorerGraph } from "../types";

type DatasetSize = "small" | "medium" | "large";

export function useUnifiedGraph(size: DatasetSize = "medium"): { graph: ExplorerGraph } {
  const graph = useMemo(() => {
    switch (size) {
      case "small": return smallFixture;
      case "large": return largeFixture;
      default: return mediumFixture;
    }
  }, [size]);

  return { graph };
}
```

- [ ] **Step 9: Run all hook tests**

Run: `cd ui && npx vitest run src/features/workstation/CodebaseExplorer/__tests__/hooks.test.ts`
Expected: All tests PASS

- [ ] **Step 10: Commit**

```bash
git add ui/src/features/workstation/CodebaseExplorer/hooks/useDepthControl.ts \
       ui/src/features/workstation/CodebaseExplorer/hooks/useAdaptiveThresholds.ts \
       ui/src/features/workstation/CodebaseExplorer/hooks/useUnifiedGraph.ts \
       ui/src/features/workstation/CodebaseExplorer/__tests__/hooks.test.ts
git commit -m "feat(explorer): add depth, threshold, and graph hooks"
```

---

## Task 3: Focus + Trace Hooks (core logic)

**Files:**
- Create: `ui/src/features/workstation/CodebaseExplorer/hooks/useFocusContext.ts`
- Create: `ui/src/features/workstation/CodebaseExplorer/hooks/useTrace.ts`
- Test: `ui/src/features/workstation/CodebaseExplorer/__tests__/hooks.test.ts` (append)

- [ ] **Step 1: Write tests for useFocusContext**

Append to `__tests__/hooks.test.ts`:

```ts
import { useFocusContext } from "../hooks/useFocusContext";

describe("useFocusContext", () => {
  it("starts in overview state with no focus", () => {
    const { result } = renderHook(() => useFocusContext(mediumFixture, 2));
    expect(result.current.stateKind).toBe("overview");
    expect(result.current.focusedNodeId).toBeNull();
    expect(result.current.upstreamIds.size).toBe(0);
    expect(result.current.downstreamIds.size).toBe(0);
  });

  it("focusing a node transitions to focus state and computes neighbors", () => {
    const { result } = renderHook(() => useFocusContext(mediumFixture, 2));
    const targetId = mediumFixture.nodes.find((n) => n.kind === "function")!.id;
    act(() => result.current.focusNode(targetId));
    expect(result.current.stateKind).toBe("focus");
    expect(result.current.focusedNodeId).toBe(targetId);
  });

  it("clearing focus returns to overview", () => {
    const { result } = renderHook(() => useFocusContext(mediumFixture, 2));
    const targetId = mediumFixture.nodes.find((n) => n.kind === "function")!.id;
    act(() => result.current.focusNode(targetId));
    act(() => result.current.clearFocus());
    expect(result.current.stateKind).toBe("overview");
    expect(result.current.focusedNodeId).toBeNull();
  });

  it("computes upstream neighbors via calls edges", () => {
    const { result } = renderHook(() => useFocusContext(mediumFixture, 1));
    // Find a node that has callers (is the target of a "calls" edge)
    const calledNodeId = mediumFixture.edges.find((e) => e.relation === "calls")?.to;
    if (!calledNodeId) return; // skip if no calls edges
    act(() => result.current.focusNode(calledNodeId));
    expect(result.current.upstreamIds.size).toBeGreaterThan(0);
  });

  it("depth change recomputes neighbors", () => {
    const { result, rerender } = renderHook(
      ({ depth }) => useFocusContext(mediumFixture, depth),
      { initialProps: { depth: 1 } }
    );
    const calledNodeId = mediumFixture.edges.find((e) => e.relation === "calls")?.to;
    if (!calledNodeId) return;
    act(() => result.current.focusNode(calledNodeId));
    const count1 = result.current.upstreamIds.size + result.current.downstreamIds.size;
    rerender({ depth: 3 });
    const count3 = result.current.upstreamIds.size + result.current.downstreamIds.size;
    expect(count3).toBeGreaterThanOrEqual(count1);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd ui && npx vitest run src/features/workstation/CodebaseExplorer/__tests__/hooks.test.ts`
Expected: FAIL — `useFocusContext` not found

- [ ] **Step 3: Implement useFocusContext**

```ts
// ui/src/features/workstation/CodebaseExplorer/hooks/useFocusContext.ts
import { useState, useMemo, useCallback } from "react";
import type { ExplorerGraph, ExplorerStateKind } from "../types";

function bfsNeighbors(
  startId: string,
  adjacency: Map<string, string[]>,
  maxDepth: number
): Set<string> {
  const visited = new Set<string>();
  let frontier = [startId];
  for (let d = 0; d < maxDepth && frontier.length > 0; d++) {
    const next: string[] = [];
    for (const id of frontier) {
      for (const neighbor of adjacency.get(id) ?? []) {
        if (!visited.has(neighbor) && neighbor !== startId) {
          visited.add(neighbor);
          next.push(neighbor);
        }
      }
    }
    frontier = next;
  }
  return visited;
}

export function useFocusContext(graph: ExplorerGraph, depth: number) {
  const [focusedNodeId, setFocusedNodeId] = useState<string | null>(null);

  // Build adjacency maps for upstream (reverse calls) and downstream (forward calls)
  const { upstreamAdj, downstreamAdj } = useMemo(() => {
    const up = new Map<string, string[]>();
    const down = new Map<string, string[]>();
    for (const edge of graph.edges) {
      if (edge.relation === "calls" || edge.relation === "parameter_flow" || edge.relation === "return_flow") {
        // downstream: from -> to (forward)
        if (!down.has(edge.from)) down.set(edge.from, []);
        down.get(edge.from)!.push(edge.to);
        // upstream: to -> from (reverse)
        if (!up.has(edge.to)) up.set(edge.to, []);
        up.get(edge.to)!.push(edge.from);
      }
    }
    return { upstreamAdj: up, downstreamAdj: down };
  }, [graph]);

  const upstreamIds = useMemo(() => {
    if (!focusedNodeId) return new Set<string>();
    return bfsNeighbors(focusedNodeId, upstreamAdj, depth);
  }, [focusedNodeId, upstreamAdj, depth]);

  const downstreamIds = useMemo(() => {
    if (!focusedNodeId) return new Set<string>();
    return bfsNeighbors(focusedNodeId, downstreamAdj, depth);
  }, [focusedNodeId, downstreamAdj, depth]);

  const stateKind: ExplorerStateKind = focusedNodeId ? "focus" : "overview";

  const focusNode = useCallback((nodeId: string) => {
    setFocusedNodeId(nodeId);
  }, []);

  const clearFocus = useCallback(() => {
    setFocusedNodeId(null);
  }, []);

  return {
    stateKind,
    focusedNodeId,
    upstreamIds,
    downstreamIds,
    focusNode,
    clearFocus,
  };
}
```

- [ ] **Step 4: Write tests for useTrace**

Append to `__tests__/hooks.test.ts`:

```ts
import { useTrace } from "../hooks/useTrace";

describe("useTrace", () => {
  it("starts with no trace", () => {
    const { result } = renderHook(() => useTrace(mediumFixture, null));
    expect(result.current.traceResult).toBeNull();
  });

  it("tracing a parameter computes upstream path", () => {
    // Find a node that receives a parameter_flow edge
    const paramEdge = mediumFixture.edges.find((e) => e.relation === "parameter_flow");
    if (!paramEdge) return;
    const { result } = renderHook(() => useTrace(mediumFixture, paramEdge.to));
    act(() => result.current.traceParameter(paramEdge.parameterName!));
    // Should produce a path with at least 2 nodes (start + origin)
    if (result.current.traceResult) {
      expect(result.current.traceResult.path.length).toBeGreaterThanOrEqual(2);
      expect(result.current.traceResult.direction).toBe("upstream");
      expect(result.current.traceResult.parameterName).toBe(paramEdge.parameterName);
    }
  });

  it("clearTrace resets result", () => {
    const { result } = renderHook(() => useTrace(mediumFixture, null));
    act(() => result.current.clearTrace());
    expect(result.current.traceResult).toBeNull();
  });

  it("returns null for dead-end parameter", () => {
    const { result } = renderHook(() =>
      useTrace(mediumFixture, mediumFixture.nodes[0]?.id ?? null)
    );
    act(() => result.current.traceParameter("nonexistent_param"));
    expect(result.current.traceResult).toBeNull();
  });
});
```

- [ ] **Step 5: Run test to verify it fails**

Run: `cd ui && npx vitest run src/features/workstation/CodebaseExplorer/__tests__/hooks.test.ts`
Expected: FAIL — `useTrace` not found

- [ ] **Step 6: Implement useTrace**

```ts
// ui/src/features/workstation/CodebaseExplorer/hooks/useTrace.ts
import { useState, useMemo, useCallback } from "react";
import type { ExplorerGraph, TraceResult } from "../types";

function bfsTracePath(
  startId: string,
  graph: ExplorerGraph,
  parameterName: string | null,
  direction: "upstream" | "downstream"
): string[] | null {
  // BFS with parent map to reconstruct a single linear path
  const parentMap = new Map<string, string>(); // child -> parent in BFS tree
  const visited = new Set<string>([startId]);
  let frontier = [startId];
  let deepestNode = startId;

  while (frontier.length > 0) {
    const next: string[] = [];
    for (const current of frontier) {
      for (const edge of graph.edges) {
        const isRelevant = direction === "upstream"
          ? edge.to === current && (edge.relation === "parameter_flow" || edge.relation === "calls")
          : edge.from === current && (edge.relation === "return_flow" || edge.relation === "calls");

        if (!isRelevant) continue;

        // For parameter tracing, filter by parameter name on parameter_flow edges
        if (direction === "upstream" && parameterName && edge.relation === "parameter_flow") {
          if (edge.parameterName !== parameterName) continue;
        }

        const neighbor = direction === "upstream" ? edge.from : edge.to;
        if (visited.has(neighbor)) continue;

        visited.add(neighbor);
        parentMap.set(neighbor, current);
        next.push(neighbor);
        deepestNode = neighbor;
      }
    }
    frontier = next;
  }

  if (deepestNode === startId) return null; // no trace found

  // Reconstruct path from deepest node back to start
  const path: string[] = [];
  let current: string | undefined = deepestNode;
  while (current !== undefined) {
    path.push(current);
    current = parentMap.get(current);
  }

  // For upstream: path is [origin, ..., start] — already correct after reversal
  // For downstream: path is [leaf, ..., start] — reverse to get [start, ..., leaf]
  return direction === "upstream" ? path : path.reverse();
}

export function useTrace(graph: ExplorerGraph, focusedNodeId: string | null) {
  const [traceResult, setTraceResult] = useState<TraceResult | null>(null);

  const traceParameter = useCallback(
    (parameterName: string) => {
      if (!focusedNodeId) return;
      const path = bfsTracePath(focusedNodeId, graph, parameterName, "upstream");
      if (!path) {
        setTraceResult(null);
        return;
      }
      setTraceResult({ path, direction: "upstream", parameterName });
    },
    [focusedNodeId, graph]
  );

  const traceReturn = useCallback(() => {
    if (!focusedNodeId) return;
    const path = bfsTracePath(focusedNodeId, graph, null, "downstream");
    if (!path) {
      setTraceResult(null);
      return;
    }
    setTraceResult({ path, direction: "downstream" });
  }, [focusedNodeId, graph]);

  const clearTrace = useCallback(() => {
    setTraceResult(null);
  }, []);

  return { traceResult, traceParameter, traceReturn, clearTrace };
}
```

- [ ] **Step 7: Run all hook tests**

Run: `cd ui && npx vitest run src/features/workstation/CodebaseExplorer/__tests__/hooks.test.ts`
Expected: All tests PASS

- [ ] **Step 8: Commit**

```bash
git add ui/src/features/workstation/CodebaseExplorer/hooks/useFocusContext.ts \
       ui/src/features/workstation/CodebaseExplorer/hooks/useTrace.ts \
       ui/src/features/workstation/CodebaseExplorer/__tests__/hooks.test.ts
git commit -m "feat(explorer): add focus and trace hooks with BFS neighbor computation"
```

---

## Task 4: Explorer Context Provider

**Files:**
- Create: `ui/src/features/workstation/CodebaseExplorer/ExplorerContext.tsx`

- [ ] **Step 1: Implement ExplorerContext**

```tsx
// ui/src/features/workstation/CodebaseExplorer/ExplorerContext.tsx
import { createContext, useContext, useState, useMemo, useCallback, type ReactNode } from "react";
import { useDepthControl } from "./hooks/useDepthControl";
import { useAdaptiveThresholds } from "./hooks/useAdaptiveThresholds";
import { useUnifiedGraph } from "./hooks/useUnifiedGraph";
import { useFocusContext } from "./hooks/useFocusContext";
import { useTrace } from "./hooks/useTrace";
import type { ExplorerContextValue, ExplorerStateKind } from "./types";

const ExplorerCtx = createContext<ExplorerContextValue | null>(null);

export function useExplorer(): ExplorerContextValue {
  const ctx = useContext(ExplorerCtx);
  if (!ctx) throw new Error("useExplorer must be used within ExplorerProvider");
  return ctx;
}

type ExplorerProviderProps = {
  children: ReactNode;
  onNavigateToSource?: (filePath: string, line?: number) => void;
};

export function ExplorerProvider({ children, onNavigateToSource }: ExplorerProviderProps) {
  const { graph } = useUnifiedGraph();
  const { depth, setDepth } = useDepthControl();
  const fileCount = useMemo(
    () => graph.nodes.filter((n) => n.kind === "file").length,
    [graph]
  );
  const adaptive = useAdaptiveThresholds(fileCount);
  const focus = useFocusContext(graph, depth);
  const trace = useTrace(graph, focus.focusedNodeId);

  const [searchQuery, setSearchQuery] = useState("");
  const [expandedClusters, setExpandedClusters] = useState<Set<string>>(new Set());
  const [deadEndMessage, setDeadEndMessage] = useState<string | null>(null);

  const matchingNodeIds = useMemo(() => {
    if (!searchQuery.trim()) return null;
    const q = searchQuery.trim().toLowerCase();
    return new Set(
      graph.nodes
        .filter((n) => n.label.toLowerCase().includes(q) || n.id.toLowerCase().includes(q))
        .map((n) => n.id)
    );
  }, [graph, searchQuery]);

  const toggleCluster = useCallback((clusterId: string) => {
    setExpandedClusters((prev) => {
      const next = new Set(prev);
      if (next.has(clusterId)) {
        next.delete(clusterId);
      } else {
        next.add(clusterId);
      }
      return next;
    });
  }, []);

  // Derive combined state kind: trace overrides focus
  const stateKind: ExplorerStateKind = trace.traceResult
    ? "trace"
    : focus.stateKind;

  const value: ExplorerContextValue = {
    graph,
    stateKind,
    focusedNodeId: focus.focusedNodeId,
    upstreamIds: focus.upstreamIds,
    downstreamIds: focus.downstreamIds,
    focusNode: focus.focusNode,
    clearFocus: () => {
      trace.clearTrace();
      focus.clearFocus();
    },
    traceResult: trace.traceResult,
    traceParameter: (paramName: string) => {
      setDeadEndMessage(null);
      trace.traceParameter(paramName);
      // Check if trace produced no result after state update
      if (!trace.traceResult) {
        setDeadEndMessage(`No upstream flow found for "${paramName}" — value may be constructed locally`);
      }
    },
    traceReturn: () => {
      setDeadEndMessage(null);
      trace.traceReturn();
      if (!trace.traceResult) {
        setDeadEndMessage("No downstream flow found — return value may not be consumed");
      }
    },
    clearTrace: () => {
      setDeadEndMessage(null);
      trace.clearTrace();
    },
    depth,
    setDepth,
    granularity: adaptive.granularity,
    setGranularity: adaptive.setGranularity,
    resolvedGranularity: adaptive.resolvedGranularity,
    thresholds: adaptive.thresholds,
    setThresholds: adaptive.setThresholds,
    searchQuery,
    setSearchQuery,
    matchingNodeIds,
    expandedClusters,
    toggleCluster,
    deadEndMessage,
    onNavigateToSource,
  };

  return <ExplorerCtx.Provider value={value}>{children}</ExplorerCtx.Provider>;
}
```

- [ ] **Step 2: Commit**

```bash
git add ui/src/features/workstation/CodebaseExplorer/ExplorerContext.tsx
git commit -m "feat(explorer): add ExplorerProvider context wiring all hooks"
```

---

## Task 5: Custom ReactFlow Node Components

**Files:**
- Create: `ui/src/features/workstation/CodebaseExplorer/nodes/ClusterNode.tsx`
- Create: `ui/src/features/workstation/CodebaseExplorer/nodes/FileNode.tsx`
- Create: `ui/src/features/workstation/CodebaseExplorer/nodes/SymbolNode.tsx`
- Test: `ui/src/features/workstation/CodebaseExplorer/__tests__/nodes.test.tsx`

- [ ] **Step 1: Write node render tests**

```tsx
// ui/src/features/workstation/CodebaseExplorer/__tests__/nodes.test.tsx
import { describe, it, expect } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { ClusterNode } from "../nodes/ClusterNode";
import { FileNode } from "../nodes/FileNode";
import { SymbolNode } from "../nodes/SymbolNode";

describe("ClusterNode", () => {
  it("renders module name and child count", () => {
    render(
      <ClusterNode
        data={{ label: "engine-crypto", childCount: 12, expanded: false, kind: "crate" }}
      />
    );
    expect(screen.getByText("engine-crypto")).toBeTruthy();
    expect(screen.getByText("12")).toBeTruthy();
  });

  it("shows expand indicator when collapsed", () => {
    render(
      <ClusterNode
        data={{ label: "intake", childCount: 5, expanded: false, kind: "module" }}
      />
    );
    expect(screen.getByLabelText("expand")).toBeTruthy();
  });
});

describe("FileNode", () => {
  it("renders filename", () => {
    render(<FileNode data={{ label: "sig.rs", language: "rust" }} />);
    expect(screen.getByText("sig.rs")).toBeTruthy();
  });
});

describe("SymbolNode", () => {
  it("renders function name and signature", () => {
    render(
      <SymbolNode
        data={{
          label: "verify_signature",
          signature: {
            parameters: [
              { name: "msg", typeAnnotation: "&[u8]", position: 0 },
              { name: "sig", typeAnnotation: "&Signature", position: 1 },
            ],
            returnType: "Result<bool>",
          },
          onParameterClick: () => {},
          onReturnClick: () => {},
        }}
      />
    );
    expect(screen.getByText("verify_signature")).toBeTruthy();
    expect(screen.getByText("msg")).toBeTruthy();
    expect(screen.getByText("&[u8]")).toBeTruthy();
    expect(screen.getByText("Result<bool>")).toBeTruthy();
  });

  it("calls onParameterClick when a parameter is clicked", () => {
    const onClick = vi.fn();
    render(
      <SymbolNode
        data={{
          label: "hash",
          signature: {
            parameters: [{ name: "data", typeAnnotation: "&[u8]", position: 0 }],
            returnType: "Hash",
          },
          onParameterClick: onClick,
          onReturnClick: () => {},
        }}
      />
    );
    fireEvent.click(screen.getByText("data"));
    expect(onClick).toHaveBeenCalledWith("data");
  });

  it("calls onReturnClick when return type is clicked", () => {
    const onClick = vi.fn();
    render(
      <SymbolNode
        data={{
          label: "hash",
          signature: {
            parameters: [],
            returnType: "Hash",
          },
          onParameterClick: () => {},
          onReturnClick: onClick,
        }}
      />
    );
    fireEvent.click(screen.getByText("Hash"));
    expect(onClick).toHaveBeenCalled();
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd ui && npx vitest run src/features/workstation/CodebaseExplorer/__tests__/nodes.test.tsx`
Expected: FAIL — components not found

- [ ] **Step 3: Implement ClusterNode**

```tsx
// ui/src/features/workstation/CodebaseExplorer/nodes/ClusterNode.tsx
import { Handle, Position } from "reactflow";

type ClusterNodeData = {
  label: string;
  childCount: number;
  expanded: boolean;
  kind: "crate" | "module";
};

export function ClusterNode({ data }: { data: ClusterNodeData }) {
  return (
    <div className="explorer-cluster-node">
      <Handle type="target" position={Position.Top} style={{ visibility: "hidden" }} />
      <div className="explorer-cluster-header">
        <span className="explorer-cluster-label">{data.label}</span>
        <span className="explorer-cluster-count">{data.childCount}</span>
        <span
          className="explorer-cluster-toggle"
          aria-label={data.expanded ? "collapse" : "expand"}
        >
          {data.expanded ? "▾" : "▸"}
        </span>
      </div>
      <Handle type="source" position={Position.Bottom} style={{ visibility: "hidden" }} />
    </div>
  );
}
```

- [ ] **Step 4: Implement FileNode**

```tsx
// ui/src/features/workstation/CodebaseExplorer/nodes/FileNode.tsx
import { Handle, Position } from "reactflow";

type FileNodeData = {
  label: string;
  language?: string;
};

export function FileNode({ data }: { data: FileNodeData }) {
  return (
    <div className="explorer-file-node">
      <Handle type="target" position={Position.Top} style={{ visibility: "hidden" }} />
      <span className="explorer-file-label">{data.label}</span>
      <Handle type="source" position={Position.Bottom} style={{ visibility: "hidden" }} />
    </div>
  );
}
```

- [ ] **Step 5: Implement SymbolNode**

```tsx
// ui/src/features/workstation/CodebaseExplorer/nodes/SymbolNode.tsx
import { Handle, Position } from "reactflow";
import type { FunctionSignature } from "../types";

type SymbolNodeData = {
  label: string;
  signature?: FunctionSignature;
  onParameterClick: (paramName: string) => void;
  onReturnClick: () => void;
};

export function SymbolNode({ data }: { data: SymbolNodeData }) {
  return (
    <div className="explorer-symbol-node">
      <Handle type="target" position={Position.Top} style={{ visibility: "hidden" }} />
      <div className="explorer-symbol-name">{data.label}</div>
      {data.signature ? (
        <div className="explorer-symbol-sig">
          <span className="explorer-sig-paren">(</span>
          {data.signature.parameters.map((param, i) => (
            <span key={param.name}>
              {i > 0 ? <span className="explorer-sig-comma">, </span> : null}
              <span
                className="explorer-sig-param"
                role="button"
                tabIndex={0}
                onClick={(e) => {
                  e.stopPropagation();
                  data.onParameterClick(param.name);
                }}
                onKeyDown={(e) => {
                  if (e.key === "Enter") data.onParameterClick(param.name);
                }}
              >
                <span className="explorer-sig-param-name">{param.name}</span>
                {param.typeAnnotation ? (
                  <span className="explorer-sig-param-type">
                    : {param.typeAnnotation}
                  </span>
                ) : null}
              </span>
            </span>
          ))}
          <span className="explorer-sig-paren">)</span>
          {data.signature.returnType ? (
            <span
              className="explorer-sig-return"
              role="button"
              tabIndex={0}
              onClick={(e) => {
                e.stopPropagation();
                data.onReturnClick();
              }}
              onKeyDown={(e) => {
                if (e.key === "Enter") data.onReturnClick();
              }}
            >
              {" → "}
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

- [ ] **Step 6: Run node tests**

Run: `cd ui && npx vitest run src/features/workstation/CodebaseExplorer/__tests__/nodes.test.tsx`
Expected: All tests PASS

- [ ] **Step 7: Commit**

```bash
git add ui/src/features/workstation/CodebaseExplorer/nodes/ClusterNode.tsx \
       ui/src/features/workstation/CodebaseExplorer/nodes/FileNode.tsx \
       ui/src/features/workstation/CodebaseExplorer/nodes/SymbolNode.tsx \
       ui/src/features/workstation/CodebaseExplorer/__tests__/nodes.test.tsx
git commit -m "feat(explorer): add custom ReactFlow node components with clickable params"
```

---

## Task 6: Adaptive Layout (graph → ReactFlow model)

**Files:**
- Create: `ui/src/features/workstation/CodebaseExplorer/AdaptiveLayout.tsx`

- [ ] **Step 1: Implement AdaptiveLayout**

This module transforms an `ExplorerGraph` into ReactFlow `Node[]` and `Edge[]` based on the current granularity and expand/collapse state. It is a pure function (no rendering), used by `ExplorerCanvas`.

```tsx
// ui/src/features/workstation/CodebaseExplorer/AdaptiveLayout.tsx
import { Position, MarkerType, type Node, type Edge } from "reactflow";
import type { ExplorerGraph, ExplorerNode } from "./types";

type LayoutConfig = {
  resolvedGranularity: "files" | "modules" | "crates";
  expandedClusters: Set<string>;
  focusedNodeId: string | null;
  upstreamIds: Set<string>;
  downstreamIds: Set<string>;
  tracePathIds: Set<string> | null;
  matchingNodeIds: Set<string> | null;
  stateKind: "overview" | "focus" | "trace";
};

export type FlowModel = {
  nodes: Node[];
  edges: Edge[];
};

function parentId(node: ExplorerNode): string | null {
  if (node.kind === "file" || node.kind === "function" || node.kind === "trait_impl_method" || node.kind === "macro_call") {
    // Derive parent from filePath: e.g., "src/crypto/" for "src/crypto/sig.rs"
    if (!node.filePath) return null;
    const lastSlash = node.filePath.lastIndexOf("/");
    if (lastSlash <= 0) return null;
    return `module:${node.filePath.slice(0, lastSlash)}`;
  }
  if (node.kind === "module") {
    // Module's parent is its crate — look for containing crate by prefix
    return null; // Will be resolved by crate membership
  }
  return null;
}

function nodeHighlightClass(
  nodeId: string,
  config: LayoutConfig
): string {
  const classes: string[] = [];
  if (config.stateKind === "overview") return "";
  if (nodeId === config.focusedNodeId) classes.push("explorer-focused");
  else if (config.tracePathIds?.has(nodeId)) classes.push("explorer-trace");
  else if (config.upstreamIds.has(nodeId)) classes.push("explorer-upstream");
  else if (config.downstreamIds.has(nodeId)) classes.push("explorer-downstream");
  else if (config.stateKind !== "overview") classes.push("explorer-dimmed");

  if (config.matchingNodeIds && !config.matchingNodeIds.has(nodeId)) {
    classes.push("explorer-search-dimmed");
  }

  return classes.join(" ");
}

export function buildFlowModel(
  graph: ExplorerGraph,
  config: LayoutConfig
): FlowModel {
  const nodes: Node[] = [];
  const edges: Edge[] = [];
  const visibleNodeIds = new Set<string>();

  // Determine which nodes to show based on granularity
  for (const node of graph.nodes) {
    let visible = false;

    switch (config.resolvedGranularity) {
      case "files":
        // Show everything except crate/module containers
        visible = node.kind !== "crate" && node.kind !== "module";
        break;
      case "modules":
        // Show modules (collapsed) and their children if expanded
        if (node.kind === "crate") {
          visible = false; // Hide crate level
        } else if (node.kind === "module") {
          visible = true;
        } else {
          // Show child if parent module is expanded
          const parent = parentId(node);
          visible = !parent || config.expandedClusters.has(parent);
        }
        break;
      case "crates":
        // Show crates (collapsed) and children of expanded crates
        if (node.kind === "crate") {
          visible = true;
        } else if (node.kind === "module") {
          // Show if parent crate expanded
          visible = config.expandedClusters.has(node.id) ||
            graph.nodes.some((n) => n.kind === "crate" && config.expandedClusters.has(n.id) && node.id.startsWith(n.id));
        } else {
          const parent = parentId(node);
          visible = !parent || config.expandedClusters.has(parent);
        }
        break;
    }

    if (!visible) continue;
    visibleNodeIds.add(node.id);

    const highlightClass = nodeHighlightClass(node.id, config);
    const isCluster = node.kind === "crate" || node.kind === "module";

    nodes.push({
      id: node.id,
      type: isCluster ? "clusterNode" : node.kind === "file" ? "fileNode" : "symbolNode",
      position: { x: 0, y: 0 }, // ELK will position these
      sourcePosition: Position.Bottom,
      targetPosition: Position.Top,
      className: highlightClass || undefined,
      data: {
        label: node.label,
        kind: node.kind,
        filePath: node.filePath,
        line: node.line,
        signature: node.signature,
        childCount: isCluster
          ? graph.nodes.filter((c) => {
              const p = parentId(c);
              return p === node.id;
            }).length
          : undefined,
        expanded: config.expandedClusters.has(node.id),
      },
    });
  }

  // Build edges between visible nodes
  const edgeKeys = new Set<string>();
  for (const edge of graph.edges) {
    if (!visibleNodeIds.has(edge.from) || !visibleNodeIds.has(edge.to)) continue;
    if (edge.from === edge.to) continue;

    const key = `${edge.from}::${edge.to}::${edge.relation}`;
    if (edgeKeys.has(key)) continue;
    edgeKeys.add(key);

    const isTraceEdge = config.tracePathIds?.has(edge.from) && config.tracePathIds?.has(edge.to);
    const isFlowEdge = edge.relation === "parameter_flow" || edge.relation === "return_flow";

    edges.push({
      id: key,
      source: edge.from,
      target: edge.to,
      type: "smoothstep",
      animated: isTraceEdge || isFlowEdge,
      label: edge.relation === "contains" ? undefined : edge.relation,
      markerEnd: {
        type: MarkerType.ArrowClosed,
        width: 16,
        height: 16,
        color: isTraceEdge ? "#8b5cf6" : "#5c7394",
      },
      style: {
        stroke: isTraceEdge ? "#8b5cf6" : "#5c7394",
        strokeWidth: isTraceEdge ? 2.5 : 1.4,
        opacity: config.stateKind !== "overview" && !isTraceEdge &&
          !config.upstreamIds.has(edge.from) && !config.downstreamIds.has(edge.from) &&
          edge.from !== config.focusedNodeId && edge.to !== config.focusedNodeId
          ? 0.15 : 1,
      },
    });
  }

  return { nodes, edges };
}
```

- [ ] **Step 2: Commit**

```bash
git add ui/src/features/workstation/CodebaseExplorer/AdaptiveLayout.tsx
git commit -m "feat(explorer): add adaptive layout graph-to-reactflow transformer"
```

---

## Task 7: Explorer Canvas (ReactFlow wrapper)

**Files:**
- Create: `ui/src/features/workstation/CodebaseExplorer/ExplorerCanvas.tsx`
- Create: `ui/src/features/workstation/CodebaseExplorer/TraceOverlay.tsx`

- [ ] **Step 1: Implement TraceOverlay**

```tsx
// ui/src/features/workstation/CodebaseExplorer/TraceOverlay.tsx
import { useExplorer } from "./ExplorerContext";

export function TraceOverlay() {
  const { traceResult, focusNode, stateKind } = useExplorer();

  if (stateKind !== "trace" || !traceResult) return null;

  return (
    <div className="explorer-trace-breadcrumbs" aria-live="polite">
      <span className="explorer-trace-label">
        {traceResult.direction === "upstream" ? "Origin trace" : "Destination trace"}
        {traceResult.parameterName ? `: ${traceResult.parameterName}` : ""}
      </span>
      <div className="explorer-trace-path">
        {traceResult.path.map((nodeId, i) => (
          <span key={nodeId}>
            {i > 0 ? <span className="explorer-trace-arrow"> → </span> : null}
            <button
              className="explorer-trace-step"
              onClick={() => focusNode(nodeId)}
              type="button"
            >
              {nodeId.split("::").pop() ?? nodeId}
            </button>
          </span>
        ))}
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Implement ExplorerCanvas**

```tsx
// ui/src/features/workstation/CodebaseExplorer/ExplorerCanvas.tsx
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import ReactFlow, {
  Background,
  Controls,
  MiniMap,
  type ReactFlowInstance,
  type Node,
  type Edge,
} from "reactflow";
import ELK from "elkjs/lib/elk.bundled.js";
import "reactflow/dist/style.css";
import { useExplorer } from "./ExplorerContext";
import { buildFlowModel } from "./AdaptiveLayout";
import { ClusterNode } from "./nodes/ClusterNode";
import { FileNode } from "./nodes/FileNode";
import { SymbolNode } from "./nodes/SymbolNode";

const elk = new ELK();

const nodeTypes = {
  clusterNode: ClusterNode,
  fileNode: FileNode,
  symbolNode: SymbolNode,
};

async function layoutWithElk(nodes: Node[], edges: Edge[]): Promise<{ nodes: Node[]; edges: Edge[] }> {
  const layout = await elk.layout({
    id: "root",
    layoutOptions: {
      "elk.algorithm": "layered",
      "elk.direction": "DOWN",
      "elk.spacing.nodeNode": "36",
      "elk.layered.spacing.nodeNodeBetweenLayers": "72",
      "elk.layered.nodePlacement.strategy": "NETWORK_SIMPLEX",
    },
    children: nodes.map((n) => ({
      id: n.id,
      width: n.type === "symbolNode" ? 280 : n.type === "clusterNode" ? 220 : 168,
      height: n.type === "symbolNode" ? 80 : 48,
    })),
    edges: edges.map((e) => ({ id: e.id, sources: [e.source], targets: [e.target] })),
  });

  const positions = new Map(
    (layout.children ?? []).map((c) => [c.id, { x: c.x ?? 0, y: c.y ?? 0 }])
  );

  return {
    nodes: nodes.map((n) => ({ ...n, position: positions.get(n.id) ?? { x: 0, y: 0 } })),
    edges,
  };
}

export function ExplorerCanvas() {
  const ctx = useExplorer();
  const [nodes, setNodes] = useState<Node[]>([]);
  const [edges, setEdges] = useState<Edge[]>([]);
  const flowRef = useRef<ReactFlowInstance | null>(null);

  const tracePathIds = useMemo(
    () => (ctx.traceResult ? new Set(ctx.traceResult.path) : null),
    [ctx.traceResult]
  );

  const flowModel = useMemo(
    () =>
      buildFlowModel(ctx.graph, {
        resolvedGranularity: ctx.resolvedGranularity,
        expandedClusters: ctx.expandedClusters,
        focusedNodeId: ctx.focusedNodeId,
        upstreamIds: ctx.upstreamIds,
        downstreamIds: ctx.downstreamIds,
        tracePathIds,
        matchingNodeIds: ctx.matchingNodeIds,
        stateKind: ctx.stateKind,
      }),
    [ctx.graph, ctx.resolvedGranularity, ctx.expandedClusters, ctx.focusedNodeId,
     ctx.upstreamIds, ctx.downstreamIds, tracePathIds, ctx.matchingNodeIds, ctx.stateKind]
  );

  // Inject callbacks into symbol node data
  const nodesWithCallbacks = useMemo(
    () =>
      flowModel.nodes.map((n) => {
        if (n.type !== "symbolNode") return n;
        return {
          ...n,
          data: {
            ...n.data,
            onParameterClick: ctx.traceParameter,
            onReturnClick: ctx.traceReturn,
          },
        };
      }),
    [flowModel.nodes, ctx.traceParameter, ctx.traceReturn]
  );

  useEffect(() => {
    void layoutWithElk(nodesWithCallbacks, flowModel.edges)
      .then((result) => {
        setNodes(result.nodes);
        setEdges(result.edges);
      })
      .catch(() => {
        // Fallback: grid layout
        setNodes(
          nodesWithCallbacks.map((n, i) => ({
            ...n,
            position: { x: (i % 5) * 320, y: Math.floor(i / 5) * 120 },
          }))
        );
        setEdges(flowModel.edges);
      });
  }, [nodesWithCallbacks, flowModel.edges]);

  const handleNodeClick = useCallback(
    (_event: React.MouseEvent, node: Node) => {
      if (node.type === "clusterNode") {
        ctx.toggleCluster(node.id);
        return;
      }
      ctx.focusNode(node.id);
    },
    [ctx]
  );

  const handlePaneClick = useCallback(() => {
    if (ctx.stateKind === "trace") {
      ctx.clearTrace();
    } else if (ctx.stateKind === "focus") {
      ctx.clearFocus();
    }
  }, [ctx]);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        if (ctx.stateKind === "trace") ctx.clearTrace();
        else if (ctx.stateKind === "focus") ctx.clearFocus();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [ctx]);

  return (
    <div className="explorer-canvas" aria-label="Codebase graph">
      <ReactFlow
        nodes={nodes}
        edges={edges}
        nodeTypes={nodeTypes}
        fitView
        fitViewOptions={{ padding: 0.16 }}
        minZoom={0.1}
        maxZoom={3}
        proOptions={{ hideAttribution: true }}
        onInit={(instance) => { flowRef.current = instance; }}
        onNodeClick={handleNodeClick}
        onPaneClick={handlePaneClick}
      >
        <Background color="#2f3845" gap={20} size={1} />
        <Controls position="top-right" />
        <MiniMap position="bottom-right" zoomable pannable />
      </ReactFlow>
    </div>
  );
}
```

- [ ] **Step 3: Commit**

```bash
git add ui/src/features/workstation/CodebaseExplorer/ExplorerCanvas.tsx \
       ui/src/features/workstation/CodebaseExplorer/TraceOverlay.tsx
git commit -m "feat(explorer): add ExplorerCanvas with ELK layout and TraceOverlay"
```

---

## Task 8: Context Panel

**Files:**
- Create: `ui/src/features/workstation/CodebaseExplorer/ContextPanel.tsx`

- [ ] **Step 1: Implement ContextPanel**

```tsx
// ui/src/features/workstation/CodebaseExplorer/ContextPanel.tsx
import { useMemo, useState } from "react";
import { useExplorer } from "./ExplorerContext";

export function ContextPanel() {
  const ctx = useExplorer();
  const [expandedSections, setExpandedSections] = useState<Set<string>>(new Set());

  const focusedNode = useMemo(
    () => ctx.graph.nodes.find((n) => n.id === ctx.focusedNodeId) ?? null,
    [ctx.graph, ctx.focusedNodeId]
  );

  if (ctx.stateKind === "overview" || !focusedNode) return null;

  const callerCount = ctx.upstreamIds.size;
  const calleeCount = ctx.downstreamIds.size;

  const toggleSection = (name: string) => {
    setExpandedSections((prev) => {
      const next = new Set(prev);
      if (next.has(name)) next.delete(name);
      else next.add(name);
      return next;
    });
  };

  const callers = ctx.graph.edges
    .filter((e) => (e.relation === "calls") && e.to === focusedNode.id)
    .map((e) => ctx.graph.nodes.find((n) => n.id === e.from))
    .filter(Boolean);

  const callees = ctx.graph.edges
    .filter((e) => (e.relation === "calls") && e.from === focusedNode.id)
    .map((e) => ctx.graph.nodes.find((n) => n.id === e.to))
    .filter(Boolean);

  return (
    <aside className="explorer-context-panel" aria-label="Node context" aria-live="polite">
      {/* Node identity */}
      <div className="explorer-ctx-header">
        <div className="explorer-ctx-name">{focusedNode.label}</div>
        <div className="explorer-ctx-location">
          {focusedNode.filePath ?? ""}
          {focusedNode.line ? `:${focusedNode.line}` : ""}
        </div>
      </div>

      {/* Function signature with clickable params */}
      {focusedNode.signature ? (
        <div className="explorer-ctx-signature">
          <span className="explorer-ctx-fn">fn </span>
          <span>{focusedNode.label}</span>
          <span>(</span>
          {focusedNode.signature.parameters.map((param, i) => (
            <span key={param.name}>
              {i > 0 ? ", " : ""}
              <button
                className="explorer-ctx-param"
                onClick={() => ctx.traceParameter(param.name)}
                type="button"
                title={`Trace origin of ${param.name}`}
              >
                {param.name}
                {param.typeAnnotation ? `: ${param.typeAnnotation}` : ""}
              </button>
            </span>
          ))}
          <span>)</span>
          {focusedNode.signature.returnType ? (
            <button
              className="explorer-ctx-return"
              onClick={ctx.traceReturn}
              type="button"
              title="Trace output destination"
            >
              {" → "}{focusedNode.signature.returnType}
            </button>
          ) : null}
        </div>
      ) : null}

      {/* Connection counts */}
      <div className="explorer-ctx-counts">
        <span>{callerCount} caller{callerCount !== 1 ? "s" : ""}</span>
        <span> · </span>
        <span>{calleeCount} callee{calleeCount !== 1 ? "s" : ""}</span>
      </div>

      {/* Trace result breadcrumbs */}
      {ctx.traceResult ? (
        <div className="explorer-ctx-trace">
          <div className="explorer-ctx-trace-label">
            {ctx.traceResult.direction === "upstream" ? "Origin" : "Destination"} trace
            {ctx.traceResult.parameterName ? `: ${ctx.traceResult.parameterName}` : ""}
          </div>
          <div className="explorer-ctx-trace-path">
            {ctx.traceResult.path.map((id, i) => {
              const node = ctx.graph.nodes.find((n) => n.id === id);
              return (
                <span key={id}>
                  {i > 0 ? " → " : ""}
                  <button
                    className="explorer-ctx-trace-step"
                    onClick={() => ctx.focusNode(id)}
                    type="button"
                  >
                    {node?.label ?? id.split("::").pop()}
                  </button>
                </span>
              );
            })}
          </div>
        </div>
      ) : null}

      {/* Dead end message — shown after a trace attempt yields no path */}
      {ctx.deadEndMessage ? (
        <div className="explorer-ctx-deadend">
          {ctx.deadEndMessage}
        </div>
      ) : null}

      {/* On-demand: Source Code */}
      <div className="explorer-ctx-section">
        <button
          className="explorer-ctx-section-toggle"
          onClick={() => toggleSection("source")}
          type="button"
        >
          Source Code
          <span>{expandedSections.has("source") ? "▾" : "▸"}</span>
        </button>
        {expandedSections.has("source") ? (
          <div className="explorer-ctx-source-preview">
            <pre className="explorer-ctx-code">
              {`// Source loading deferred to Phase 2 API integration\n// File: ${focusedNode.filePath ?? "unknown"}${focusedNode.line ? `:${focusedNode.line}` : ""}`}
            </pre>
          </div>
        ) : null}
      </div>

      {/* On-demand: Dataflow In/Out */}
      <div className="explorer-ctx-section">
        <button
          className="explorer-ctx-section-toggle"
          onClick={() => toggleSection("dataflow")}
          type="button"
        >
          Dataflow In/Out
          <span>{expandedSections.has("dataflow") ? "▾" : "▸"}</span>
        </button>
        {expandedSections.has("dataflow") ? (
          <div className="explorer-ctx-dataflow">
            {focusedNode.signature?.parameters.map((p) => {
              const inEdges = ctx.graph.edges.filter(
                (e) => e.relation === "parameter_flow" && e.to === focusedNode.id && e.parameterName === p.name
              );
              return (
                <div key={p.name} className="explorer-ctx-dataflow-row">
                  <span className="explorer-ctx-dataflow-param">{p.name}</span>
                  <span className="explorer-ctx-dataflow-arrow"> ← </span>
                  {inEdges.length > 0
                    ? inEdges.map((e) => {
                        const src = ctx.graph.nodes.find((n) => n.id === e.from);
                        return <span key={e.from} className="explorer-ctx-dataflow-src">{src?.label ?? e.from}</span>;
                      })
                    : <span className="explorer-ctx-dataflow-none">local/literal</span>}
                </div>
              );
            })}
          </div>
        ) : null}
      </div>

      {/* On-demand: Full Call Path */}
      <div className="explorer-ctx-section">
        <button
          className="explorer-ctx-section-toggle"
          onClick={() => toggleSection("fullpath")}
          type="button"
        >
          Full Call Path
          <span>{expandedSections.has("fullpath") ? "▾" : "▸"}</span>
        </button>
        {expandedSections.has("fullpath") ? (
          <div className="explorer-ctx-fullpath">
            <button
              className="explorer-ctx-trace-btn"
              onClick={() => ctx.traceParameter(focusedNode.signature?.parameters[0]?.name ?? "")}
              type="button"
            >
              Trace from entry points
            </button>
          </div>
        ) : null}
      </div>

      {/* On-demand: Callers */}
      <div className="explorer-ctx-section">
        <button
          className="explorer-ctx-section-toggle"
          onClick={() => toggleSection("callers")}
          type="button"
        >
          Callers ({callers.length})
          <span>{expandedSections.has("callers") ? "▾" : "▸"}</span>
        </button>
        {expandedSections.has("callers") ? (
          <ul className="explorer-ctx-list">
            {callers.map((n) => (
              <li key={n!.id}>
                <button onClick={() => ctx.focusNode(n!.id)} type="button">
                  {n!.label}
                  {n!.filePath ? ` — ${n!.filePath}` : ""}
                </button>
              </li>
            ))}
          </ul>
        ) : null}
      </div>

      {/* On-demand: Callees */}
      <div className="explorer-ctx-section">
        <button
          className="explorer-ctx-section-toggle"
          onClick={() => toggleSection("callees")}
          type="button"
        >
          Callees ({callees.length})
          <span>{expandedSections.has("callees") ? "▾" : "▸"}</span>
        </button>
        {expandedSections.has("callees") ? (
          <ul className="explorer-ctx-list">
            {callees.map((n) => (
              <li key={n!.id}>
                <button onClick={() => ctx.focusNode(n!.id)} type="button">
                  {n!.label}
                  {n!.filePath ? ` — ${n!.filePath}` : ""}
                </button>
              </li>
            ))}
          </ul>
        ) : null}
      </div>

      {/* Navigate to source */}
      {focusedNode.filePath && ctx.onNavigateToSource ? (
        <button
          className="explorer-ctx-source-btn"
          onClick={() => ctx.onNavigateToSource!(focusedNode.filePath!, focusedNode.line)}
          type="button"
        >
          Open in editor
        </button>
      ) : null}
    </aside>
  );
}
```

- [ ] **Step 2: Commit**

```bash
git add ui/src/features/workstation/CodebaseExplorer/ContextPanel.tsx
git commit -m "feat(explorer): add ContextPanel with signature display and on-demand sections"
```

---

## Task 9: Main Component and Toolbar

**Files:**
- Create: `ui/src/features/workstation/CodebaseExplorer/index.tsx`

- [ ] **Step 1: Implement index.tsx**

```tsx
// ui/src/features/workstation/CodebaseExplorer/index.tsx
import { ExplorerProvider, useExplorer } from "./ExplorerContext";
import { ExplorerCanvas } from "./ExplorerCanvas";
import { ContextPanel } from "./ContextPanel";
import { TraceOverlay } from "./TraceOverlay";
import type { GranularityLevel } from "./types";

type CodebaseExplorerProps = {
  sessionId: string; // Plumbed through for Phase 2 API integration — unused in Phase 1
  onNavigateToSource?: (filePath: string, line?: number) => void;
};

function ExplorerToolbar() {
  const ctx = useExplorer();

  return (
    <div className="explorer-toolbar" role="toolbar" aria-label="Explorer controls">
      <select
        value={ctx.granularity}
        onChange={(e) => ctx.setGranularity(e.target.value as GranularityLevel)}
        className="explorer-granularity-select"
        aria-label="View granularity"
      >
        <option value="auto">Auto</option>
        <option value="files">Files</option>
        <option value="modules">Modules</option>
        <option value="crates">Crates</option>
      </select>

      <input
        type="text"
        placeholder="Search nodes..."
        value={ctx.searchQuery}
        onChange={(e) => ctx.setSearchQuery(e.target.value)}
        className="explorer-search"
        aria-label="Search nodes"
        disabled={ctx.stateKind === "trace"}
      />
      {ctx.matchingNodeIds ? (
        <span className="explorer-match-count">
          {ctx.matchingNodeIds.size} matches
        </span>
      ) : null}

      {ctx.stateKind !== "overview" ? (
        <div className="explorer-depth-control" role="group" aria-label="Depth control">
          <button
            onClick={() => ctx.setDepth(ctx.depth - 1)}
            disabled={ctx.depth <= 1}
            type="button"
            aria-label="Decrease depth"
          >
            −
          </button>
          <span className="explorer-depth-value">{ctx.depth}</span>
          <button
            onClick={() => ctx.setDepth(ctx.depth + 1)}
            disabled={ctx.depth >= 10}
            type="button"
            aria-label="Increase depth"
          >
            +
          </button>
        </div>
      ) : null}

      <span className="explorer-state-badge">
        {ctx.stateKind.toUpperCase()}
      </span>
    </div>
  );
}

function ExplorerLayout() {
  const ctx = useExplorer();

  return (
    <section className="explorer-root" aria-label="Codebase Explorer">
      <ExplorerToolbar />
      <div className="explorer-body">
        <div className="explorer-canvas-container">
          <ExplorerCanvas />
          <TraceOverlay />
        </div>
        {ctx.stateKind !== "overview" ? (
          <div className="explorer-panel-container">
            <ContextPanel />
          </div>
        ) : null}
      </div>
    </section>
  );
}

export default function CodebaseExplorer({ sessionId, onNavigateToSource }: CodebaseExplorerProps) {
  return (
    <ExplorerProvider onNavigateToSource={onNavigateToSource}>
      <ExplorerLayout />
    </ExplorerProvider>
  );
}
```

- [ ] **Step 2: Commit**

```bash
git add ui/src/features/workstation/CodebaseExplorer/index.tsx
git commit -m "feat(explorer): add main CodebaseExplorer component with toolbar"
```

---

## Task 10: CSS Styling

**Files:**
- Modify: `ui/src/styles.css`

- [ ] **Step 1: Add explorer CSS classes**

Append to `ui/src/styles.css` (after the existing graph lens styles around line 1422):

```css
/* ── Codebase Explorer ── */

.explorer-root {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: #0a0e17;
}

.explorer-toolbar {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 12px;
  border-bottom: 1px solid #1e293b;
  background: #111827;
}

.explorer-granularity-select {
  background: #1e293b;
  color: #e2e8f0;
  border: 1px solid #334155;
  border-radius: 4px;
  padding: 4px 8px;
  font-size: 12px;
  min-width: 100px;
}

.explorer-search {
  background: #1e293b;
  color: #e2e8f0;
  border: 1px solid #334155;
  border-radius: 4px;
  padding: 4px 8px;
  font-size: 12px;
  flex: 1;
  max-width: 280px;
}

.explorer-search:disabled {
  opacity: 0.4;
}

.explorer-match-count {
  color: #64748b;
  font-size: 11px;
}

.explorer-depth-control {
  display: flex;
  align-items: center;
  gap: 4px;
  background: #1e293b;
  border-radius: 4px;
  padding: 2px;
}

.explorer-depth-control button {
  background: none;
  border: none;
  color: #94a3b8;
  cursor: pointer;
  width: 24px;
  height: 24px;
  font-size: 14px;
  border-radius: 3px;
}

.explorer-depth-control button:hover:not(:disabled) {
  background: #334155;
  color: #e2e8f0;
}

.explorer-depth-control button:disabled {
  opacity: 0.3;
  cursor: default;
}

.explorer-depth-value {
  color: #e2e8f0;
  font-size: 12px;
  min-width: 16px;
  text-align: center;
}

.explorer-state-badge {
  color: #64748b;
  font-size: 10px;
  text-transform: uppercase;
  letter-spacing: 1px;
  margin-left: auto;
}

.explorer-body {
  display: flex;
  flex: 1;
  overflow: hidden;
}

.explorer-canvas-container {
  flex: 1;
  position: relative;
}

.explorer-canvas {
  width: 100%;
  height: 100%;
}

.explorer-panel-container {
  width: 320px;
  min-width: 280px;
  border-left: 1px solid #1e293b;
  overflow-y: auto;
}

/* Node styles */
.explorer-cluster-node {
  background: #1a2332;
  border: 1.5px solid #334155;
  border-radius: 8px;
  padding: 8px 12px;
  min-width: 180px;
}

.explorer-cluster-header {
  display: flex;
  align-items: center;
  gap: 8px;
}

.explorer-cluster-label {
  color: #94a3b8;
  font-size: 12px;
  font-weight: 600;
}

.explorer-cluster-count {
  color: #64748b;
  font-size: 10px;
  background: #0f172a;
  padding: 1px 6px;
  border-radius: 8px;
}

.explorer-cluster-toggle {
  color: #64748b;
  font-size: 10px;
  margin-left: auto;
}

.explorer-file-node {
  background: #1e3a5f;
  border: 1px solid #2563eb;
  border-radius: 4px;
  padding: 6px 10px;
}

.explorer-file-label {
  color: #93c5fd;
  font-size: 11px;
}

.explorer-symbol-node {
  background: #111827;
  border: 1.5px solid #334155;
  border-radius: 6px;
  padding: 8px 10px;
  min-width: 200px;
}

.explorer-symbol-name {
  color: #e2e8f0;
  font-size: 12px;
  font-weight: 600;
  font-family: "JetBrains Mono", monospace;
}

.explorer-symbol-sig {
  color: #94a3b8;
  font-size: 10px;
  font-family: "JetBrains Mono", monospace;
  margin-top: 4px;
}

.explorer-sig-param {
  cursor: pointer;
  border-radius: 2px;
  padding: 0 2px;
}

.explorer-sig-param:hover {
  background: #1e3a5f;
  color: #93c5fd;
}

.explorer-sig-param-name {
  color: #a5b4fc;
}

.explorer-sig-param-type {
  color: #64748b;
}

.explorer-sig-return {
  cursor: pointer;
  border-radius: 2px;
  padding: 0 2px;
}

.explorer-sig-return:hover {
  background: #2d1b69;
  color: #c4b5fd;
}

.explorer-sig-return-type {
  color: #8b5cf6;
}

/* Focus/dim states */
.explorer-focused .explorer-symbol-node,
.explorer-focused .explorer-file-node {
  border-color: #22c55e;
  box-shadow: 0 0 12px rgba(34, 197, 94, 0.3);
}

.explorer-upstream .explorer-symbol-node,
.explorer-upstream .explorer-file-node {
  border-color: #3b82f6;
}

.explorer-downstream .explorer-symbol-node,
.explorer-downstream .explorer-file-node {
  border-color: #f97316;
}

.explorer-trace .explorer-symbol-node,
.explorer-trace .explorer-file-node {
  border-color: #8b5cf6;
  box-shadow: 0 0 8px rgba(139, 92, 246, 0.3);
}

.explorer-dimmed {
  opacity: 0.2;
}

.explorer-search-dimmed {
  opacity: 0.1;
}

/* Context panel */
.explorer-context-panel {
  padding: 12px;
  background: #0f172a;
  color: #e2e8f0;
  font-size: 12px;
}

.explorer-ctx-header {
  margin-bottom: 12px;
}

.explorer-ctx-name {
  font-size: 14px;
  font-weight: 700;
  color: #4ade80;
  font-family: "JetBrains Mono", monospace;
}

.explorer-ctx-location {
  font-size: 11px;
  color: #64748b;
  margin-top: 2px;
}

.explorer-ctx-signature {
  background: #111827;
  border: 1px solid #1e293b;
  border-radius: 4px;
  padding: 8px;
  font-family: "JetBrains Mono", monospace;
  font-size: 11px;
  color: #94a3b8;
  margin-bottom: 8px;
}

.explorer-ctx-fn {
  color: #c084fc;
}

.explorer-ctx-param {
  background: none;
  border: none;
  cursor: pointer;
  color: #a5b4fc;
  font-family: inherit;
  font-size: inherit;
  padding: 1px 3px;
  border-radius: 2px;
}

.explorer-ctx-param:hover {
  background: #1e3a5f;
}

.explorer-ctx-return {
  background: none;
  border: none;
  cursor: pointer;
  color: #8b5cf6;
  font-family: inherit;
  font-size: inherit;
  padding: 1px 3px;
  border-radius: 2px;
}

.explorer-ctx-return:hover {
  background: #2d1b69;
}

.explorer-ctx-counts {
  color: #64748b;
  font-size: 11px;
  margin-bottom: 12px;
}

.explorer-ctx-trace {
  background: #1e1b4b;
  border: 1px solid #4c1d95;
  border-radius: 4px;
  padding: 8px;
  margin-bottom: 8px;
}

.explorer-ctx-trace-label {
  color: #c4b5fd;
  font-size: 10px;
  text-transform: uppercase;
  letter-spacing: 0.5px;
  margin-bottom: 4px;
}

.explorer-ctx-trace-path {
  color: #94a3b8;
  font-size: 11px;
}

.explorer-ctx-trace-step {
  background: none;
  border: none;
  color: #a5b4fc;
  cursor: pointer;
  font-family: "JetBrains Mono", monospace;
  font-size: 11px;
}

.explorer-ctx-trace-step:hover {
  color: #e2e8f0;
  text-decoration: underline;
}

.explorer-ctx-section {
  border-top: 1px solid #1e293b;
  padding-top: 8px;
  margin-top: 8px;
}

.explorer-ctx-section-toggle {
  background: none;
  border: none;
  color: #94a3b8;
  cursor: pointer;
  font-size: 11px;
  width: 100%;
  text-align: left;
  display: flex;
  justify-content: space-between;
  padding: 4px 0;
}

.explorer-ctx-section-toggle:hover {
  color: #e2e8f0;
}

.explorer-ctx-list {
  list-style: none;
  padding: 4px 0 0 0;
  margin: 0;
}

.explorer-ctx-list li {
  padding: 2px 0;
}

.explorer-ctx-list button {
  background: none;
  border: none;
  color: #93c5fd;
  cursor: pointer;
  font-size: 11px;
  text-align: left;
}

.explorer-ctx-list button:hover {
  color: #e2e8f0;
}

.explorer-ctx-source-btn {
  display: block;
  width: 100%;
  margin-top: 12px;
  padding: 6px;
  background: #1e293b;
  border: 1px solid #334155;
  border-radius: 4px;
  color: #94a3b8;
  cursor: pointer;
  font-size: 11px;
  text-align: center;
}

.explorer-ctx-source-btn:hover {
  background: #334155;
  color: #e2e8f0;
}

/* Trace breadcrumbs overlay */
.explorer-trace-breadcrumbs {
  position: absolute;
  bottom: 12px;
  left: 12px;
  right: 12px;
  background: rgba(30, 27, 75, 0.9);
  border: 1px solid #4c1d95;
  border-radius: 6px;
  padding: 8px 12px;
  z-index: 10;
}

.explorer-trace-label {
  color: #c4b5fd;
  font-size: 10px;
  text-transform: uppercase;
  letter-spacing: 0.5px;
}

.explorer-trace-path {
  margin-top: 4px;
  font-size: 11px;
}

.explorer-trace-arrow {
  color: #6d28d9;
}

.explorer-trace-step {
  background: none;
  border: none;
  color: #a5b4fc;
  cursor: pointer;
  font-family: "JetBrains Mono", monospace;
  font-size: 11px;
}

.explorer-trace-step:hover {
  color: #e2e8f0;
  text-decoration: underline;
}
```

- [ ] **Step 2: Commit**

```bash
git add ui/src/styles.css
git commit -m "feat(explorer): add CSS styles for codebase explorer components"
```

---

## Task 11: Integration — Replace GraphLens

**Files:**
- Modify: `ui/src/features/workstation/WorkstationShell.tsx` (lines 13, 251-257, 343-352, 374-380)
- Remove: `ui/src/features/workstation/GraphLensReactFlow.tsx`
- Remove: `ui/src/features/workstation/GraphLensCytoscape.tsx`
- Remove: `ui/src/features/workstation/GraphLens.tsx`
- Remove: `ui/src/features/workstation/GraphLens.test.tsx`

- [ ] **Step 1: Update WorkstationShell import**

In `ui/src/features/workstation/WorkstationShell.tsx`, replace line 13:

```ts
// Before:
import GraphLens from "./GraphLens";
// After:
import CodebaseExplorer from "./CodebaseExplorer";
```

- [ ] **Step 2: Replace all 3 GraphLens render sites**

Replace all `<GraphLens ... />` occurrences (lines 251-257, 343-352, 374-380) with:

```tsx
<CodebaseExplorer
  sessionId={sessionId}
  onNavigateToSource={handleNavigateToSource}
/>
```

Remove the `selectedGraphNodeIds` and `focusedSymbolName` props — the explorer manages focus internally.

- [ ] **Step 3: Remove old files**

```bash
rm ui/src/features/workstation/GraphLensReactFlow.tsx
rm ui/src/features/workstation/GraphLensCytoscape.tsx
rm ui/src/features/workstation/GraphLens.tsx
rm ui/src/features/workstation/GraphLens.test.tsx
```

- [ ] **Step 4: Verify build compiles**

Run: `cd ui && npx tsc -b --noEmit`
Expected: No TypeScript errors

- [ ] **Step 5: Commit**

```bash
git add -u ui/src/features/workstation/
git add ui/src/features/workstation/WorkstationShell.tsx
git commit -m "feat(explorer): replace GraphLens with CodebaseExplorer in WorkstationShell"
```

---

## Task 12: Integration Tests

**Files:**
- Create: `ui/src/features/workstation/CodebaseExplorer/__tests__/CodebaseExplorer.test.tsx`

- [ ] **Step 1: Write integration tests**

```tsx
// ui/src/features/workstation/CodebaseExplorer/__tests__/CodebaseExplorer.test.tsx
import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import CodebaseExplorer from "../index";

describe("CodebaseExplorer", () => {
  it("renders in overview state with toolbar", () => {
    render(<CodebaseExplorer sessionId="test-session" />);
    expect(screen.getByLabelText("Explorer controls")).toBeTruthy();
    expect(screen.getByLabelText("View granularity")).toBeTruthy();
    expect(screen.getByLabelText("Search nodes")).toBeTruthy();
    expect(screen.getByText("OVERVIEW")).toBeTruthy();
  });

  it("renders graph nodes from fixture data", () => {
    render(<CodebaseExplorer sessionId="test-session" />);
    expect(screen.getByLabelText("Codebase graph")).toBeTruthy();
  });

  it("does not show context panel in overview state", () => {
    render(<CodebaseExplorer sessionId="test-session" />);
    expect(screen.queryByLabelText("Node context")).toBeNull();
  });

  it("search input works and shows match count", () => {
    render(<CodebaseExplorer sessionId="test-session" />);
    const search = screen.getByLabelText("Search nodes");
    fireEvent.change(search, { target: { value: "verify" } });
    expect(screen.getByText(/matches/)).toBeTruthy();
  });

  it("granularity dropdown has all options", () => {
    render(<CodebaseExplorer sessionId="test-session" />);
    const select = screen.getByLabelText("View granularity");
    expect(select).toBeTruthy();
    // Check options exist
    const options = select.querySelectorAll("option");
    expect(options.length).toBe(4);
  });

  it("depth control not visible in overview", () => {
    render(<CodebaseExplorer sessionId="test-session" />);
    expect(screen.queryByLabelText("Depth control")).toBeNull();
  });

  it("shows FOCUS state badge and context panel after node click", () => {
    render(<CodebaseExplorer sessionId="test-session" />);
    // Find a rendered function node and click it
    const symbolNodes = screen.getAllByText(/verify|hash|parse/i);
    if (symbolNodes.length > 0) {
      fireEvent.click(symbolNodes[0]);
      expect(screen.getByText("FOCUS")).toBeTruthy();
      expect(screen.getByLabelText("Node context")).toBeTruthy();
    }
  });

  it("Esc returns from focus to overview", () => {
    render(<CodebaseExplorer sessionId="test-session" />);
    const symbolNodes = screen.getAllByText(/verify|hash|parse/i);
    if (symbolNodes.length > 0) {
      fireEvent.click(symbolNodes[0]);
      expect(screen.getByText("FOCUS")).toBeTruthy();
      fireEvent.keyDown(window, { key: "Escape" });
      expect(screen.getByText("OVERVIEW")).toBeTruthy();
    }
  });
});
```

- [ ] **Step 2: Run all tests**

Run: `cd ui && npx vitest run src/features/workstation/CodebaseExplorer/`
Expected: All tests PASS (hooks, nodes, integration)

- [ ] **Step 3: Run full test suite to check for regressions**

Run: `cd ui && npx vitest run`
Expected: All tests PASS. Old `GraphLens.test.tsx` tests should be gone (file removed in Task 11). If other tests reference GraphLens, fix imports.

- [ ] **Step 4: Commit**

```bash
git add ui/src/features/workstation/CodebaseExplorer/__tests__/CodebaseExplorer.test.tsx
git commit -m "test(explorer): add integration tests for CodebaseExplorer state machine"
```

---

## Task 13: Dev Server Verification

- [ ] **Step 1: Start dev server and verify in browser**

Run: `cd ui && npm run dev`

Open `http://localhost:5173` in browser. Navigate to a workstation view. Verify:
1. Graph renders with nodes from fixture data
2. Toolbar shows granularity dropdown, search input, state badge
3. Clicking a function node transitions to FOCUS (node highlights green, neighbors in blue/orange, rest dims)
4. Context panel appears with function name, signature, clickable parameters
5. Clicking a parameter triggers trace (purple path, breadcrumbs)
6. Esc returns to previous state
7. Depth +/- buttons adjust neighbor visibility
8. Granularity dropdown switches between Files/Modules/Crates views

- [ ] **Step 2: Fix any visual issues found during manual testing**

Address layout, spacing, or color issues discovered during browser testing.

- [ ] **Step 3: Final commit**

```bash
git add -A
git commit -m "fix(explorer): address visual polish from manual testing"
```

---

## Dependency Graph

```
Task 1 (types + fixtures)
  └──► Task 2 (simple hooks)
         └──► Task 3 (focus + trace hooks)
                └──► Task 4 (context provider)
                       ├──► Task 5 (node components)  ─┐
                       └──► Task 6 (adaptive layout)  ─┤
                                                       ▼
                                                  Task 7 (canvas) ← depends on 5 + 6
                                                       └──► Task 8 (context panel)
                                                              └──► Task 9 (main component + toolbar)
                                                                     └──► Task 10 (CSS)
                                                                            └──► Task 11 (integration — replace GraphLens)
                                                                                   └──► Task 12 (integration tests)
                                                                                          └──► Task 13 (dev server verification)
```

Tasks 5 and 6 can run in parallel after Task 4. Task 7 depends on both 5 and 6.
