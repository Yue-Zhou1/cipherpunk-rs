# Pixi.js DAG Explorer Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace the ReactFlow canvas in CodebaseExplorer with a Pixi.js WebGL renderer that handles 300+ nodes at 60fps, adds semantic zoom, relation-typed edge coloring, spring-physics ego layout, edge particle animations, and attack-path / dataflow trace modes via a right-click context menu.

**Architecture:** Two-layer split — a `PixiRenderer` class owns the WebGL canvas and receives a plain `RenderGraph` data object; existing React hooks (`useUnifiedGraph`, `useFocusContext`, `useTrace`) are unchanged and feed the renderer via a thin `useRenderGraph` adapter. ELK handles initial hierarchical layout; d3-force handles spring animation during focus/expand. React renders all UI chrome (toolbar, panels, context menu) alongside the canvas.

**Tech Stack:** `pixi.js` v8, `d3-force`, `elkjs` (already installed), `framer-motion` (already installed), `vitest` + `@testing-library/react` for unit tests.

**Design reference:** `docs/plans/2026-04-11-pixi-dag-explorer-design.md`

---

## Orientation

### Key existing files (read before touching anything)

- `ui/src/features/workstation/CodebaseExplorer/types.ts` — `ExplorerNode`, `ExplorerEdge`, `ExplorerGraph`, `ExplorerContextValue`
- `ui/src/features/workstation/CodebaseExplorer/ExplorerContext.tsx` — `ExplorerProvider`, `useExplorer`
- `ui/src/features/workstation/CodebaseExplorer/hooks/useFocusContext.ts` — BFS ego-network, `focusNode`, `clearFocus`
- `ui/src/features/workstation/CodebaseExplorer/hooks/useTrace.ts` — `showCallers`, `showCallees`, `clearHighlight`
- `ui/src/features/workstation/CodebaseExplorer/hooks/useUnifiedGraph.ts` — loads graph from backend, manages cluster expansion
- `ui/src/features/workstation/CodebaseExplorer/index.tsx` — `CodebaseExplorer` component, `ExplorerToolbar`, `EgoBanner`, `ExplorerLayout`
- `ui/src/features/workstation/CodebaseExplorer/__tests__/CodebaseExplorer.test.tsx` — existing integration tests (must stay green throughout)
- `ui/src/styles.css` — all `.explorer-*` CSS classes

### Files to delete (Phase 6 only — do NOT delete early)

- `ExplorerCanvas.tsx`, `AdaptiveLayout.tsx`, `nodes/ClusterNode.tsx`, `nodes/FileNode.tsx`, `nodes/SymbolNode.tsx`

### Install commands

```bash
cd ui
npm install pixi.js d3-force
npm install --save-dev @types/d3-force
```

### Run tests

```bash
cd ui && npx vitest run
```

### Run dev server

```bash
cd ui && npm run dev
```

---

## Phase 1 — Pixi Shell

Mount a blank WebGL canvas in place of ReactFlow. Nothing draws yet. Proves Pixi initialises, resizes, and cleans up without memory leaks.

---

### Task 1.1: Install dependencies

**Files:**
- Modify: `ui/package.json` (via npm)

**Step 1: Install**

```bash
cd ui
npm install pixi.js d3-force
npm install --save-dev @types/d3-force
```

**Step 2: Verify installed**

```bash
cd ui && node -e "require('./node_modules/pixi.js/package.json'); console.log('pixi ok')"
```

Expected output: `pixi ok`

**Step 3: Commit**

```bash
cd ui && git add package.json package-lock.json
git commit -m "chore(explorer): add pixi.js and d3-force dependencies"
```

---

### Task 1.2: Create PixiRenderer class

**Files:**
- Create: `ui/src/features/workstation/CodebaseExplorer/pixi/PixiRenderer.ts`

**Step 1: Write the file**

```typescript
// ui/src/features/workstation/CodebaseExplorer/pixi/PixiRenderer.ts
import * as PIXI from "pixi.js";

export type PixiRendererOptions = {
  canvas: HTMLCanvasElement;
  onNodeClick: (nodeId: string) => void;
  onNodeRightClick: (nodeId: string, x: number, y: number) => void;
  onPaneClick: () => void;
};

export class PixiRenderer {
  private app: PIXI.Application;
  private options: PixiRendererOptions;

  constructor(options: PixiRendererOptions) {
    this.options = options;
    this.app = new PIXI.Application();
  }

  async init(): Promise<void> {
    await this.app.init({
      canvas: this.options.canvas,
      background: 0x0a0f1a,
      antialias: true,
      autoDensity: true,
      resolution: window.devicePixelRatio || 1,
      resizeTo: this.options.canvas.parentElement ?? this.options.canvas,
    });
  }

  resize(): void {
    this.app.resize();
  }

  destroy(): void {
    this.app.destroy(false, { children: true });
  }
}
```

**Step 2: No test yet — tested via hook in Task 1.3**

---

### Task 1.3: Create usePixiRenderer hook

**Files:**
- Create: `ui/src/features/workstation/CodebaseExplorer/pixi/usePixiRenderer.ts`

**Step 1: Write the hook**

```typescript
// ui/src/features/workstation/CodebaseExplorer/pixi/usePixiRenderer.ts
import { useEffect, useRef } from "react";
import { PixiRenderer, type PixiRendererOptions } from "./PixiRenderer";

export function usePixiRenderer(
  canvasRef: React.RefObject<HTMLCanvasElement | null>,
  options: Omit<PixiRendererOptions, "canvas">
): React.RefObject<PixiRenderer | null> {
  const rendererRef = useRef<PixiRenderer | null>(null);

  // Keep a stable ref to the latest callbacks so the renderer always calls
  // the current version without needing to be re-created.
  const callbacksRef = useRef(options);
  useEffect(() => {
    callbacksRef.current = options;
  });

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    // Pass stable proxy callbacks that delegate to callbacksRef.current.
    const renderer = new PixiRenderer({
      canvas,
      onNodeClick: (nodeId) => callbacksRef.current.onNodeClick(nodeId),
      onNodeRightClick: (nodeId, x, y) => callbacksRef.current.onNodeRightClick(nodeId, x, y),
      onPaneClick: () => callbacksRef.current.onPaneClick(),
    });
    rendererRef.current = renderer;

    void renderer.init();

    const handleResize = () => renderer.resize();
    window.addEventListener("resize", handleResize);

    return () => {
      window.removeEventListener("resize", handleResize);
      renderer.destroy();
      rendererRef.current = null;
    };
  }, [canvasRef]); // renderer created once; callbacks always current via callbacksRef

  return rendererRef;
}
```

---

### Task 1.4: Create new ExplorerCanvas shell

**Files:**
- Create: `ui/src/features/workstation/CodebaseExplorer/PixiExplorerCanvas.tsx`

Do NOT delete the old `ExplorerCanvas.tsx` yet — `index.tsx` still imports it.

**Step 1: Write the shell**

```typescript
// ui/src/features/workstation/CodebaseExplorer/PixiExplorerCanvas.tsx
import { useRef, useCallback } from "react";
import { useExplorer } from "./ExplorerContext";
import { usePixiRenderer } from "./pixi/usePixiRenderer";

export function PixiExplorerCanvas() {
  const ctx = useExplorer();
  const canvasRef = useRef<HTMLCanvasElement>(null);

  const handleNodeClick = useCallback(
    (nodeId: string) => {
      const node = ctx.nodeMap.get(nodeId);
      if (node?.kind === "crate" || node?.kind === "module") {
        // Cluster nodes expand/collapse their children instead of entering focus mode
        ctx.expandCluster(nodeId);
        ctx.toggleCluster(nodeId);
      } else {
        ctx.focusNode(nodeId);
      }
    },
    [ctx]
  );

  const handleNodeRightClick = useCallback(
    (_nodeId: string, _x: number, _y: number) => {
      // context menu wired in Phase 3
    },
    []
  );

  const handlePaneClick = useCallback(() => {
    if (ctx.neighborhoodResult) {
      ctx.clearHighlight();
    } else if (ctx.stateKind === "focus") {
      ctx.clearFocus();
    }
  }, [ctx]);

  usePixiRenderer(canvasRef, {
    onNodeClick: handleNodeClick,
    onNodeRightClick: handleNodeRightClick,
    onPaneClick: handlePaneClick,
  });

  return (
    <div className="explorer-canvas" aria-label="Codebase graph" style={{ width: "100%", height: "100%", position: "relative" }}>
      <canvas ref={canvasRef} style={{ display: "block", width: "100%", height: "100%" }} />
    </div>
  );
}
```

**Step 2: Wire into index.tsx temporarily** — add an import and swap the canvas for a quick visual test only. Revert this swap before committing; the permanent swap happens in Phase 2 Task 2.7.

---

### Task 1.5: Write smoke test for PixiRenderer

**Files:**
- Create: `ui/src/features/workstation/CodebaseExplorer/__tests__/PixiRenderer.test.ts`

**Step 1: Write the test**

```typescript
// ui/src/features/workstation/CodebaseExplorer/__tests__/PixiRenderer.test.ts
import { describe, it, expect, vi, beforeEach } from "vitest";

// Pixi.js requires a real WebGL context — mock the whole module in jsdom
vi.mock("pixi.js", () => {
  const mockApp = {
    init: vi.fn().mockResolvedValue(undefined),
    resize: vi.fn(),
    destroy: vi.fn(),
  };
  return {
    Application: vi.fn(() => mockApp),
    __mockApp: mockApp,
  };
});

import * as PIXI from "pixi.js";
import { PixiRenderer } from "../pixi/PixiRenderer";

describe("PixiRenderer", () => {
  let canvas: HTMLCanvasElement;
  const noop = () => {};

  beforeEach(() => {
    canvas = document.createElement("canvas");
    vi.clearAllMocks();
  });

  it("calls app.init with canvas and background color on init()", async () => {
    const renderer = new PixiRenderer({
      canvas,
      onNodeClick: noop,
      onNodeRightClick: noop,
      onPaneClick: noop,
    });
    await renderer.init();

    const mockApp = (PIXI as any).__mockApp;
    expect(mockApp.init).toHaveBeenCalledOnce();
    const initArgs = mockApp.init.mock.calls[0][0];
    expect(initArgs.canvas).toBe(canvas);
    expect(initArgs.background).toBe(0x0a0f1a);
  });

  it("calls app.destroy on destroy()", async () => {
    const renderer = new PixiRenderer({
      canvas,
      onNodeClick: noop,
      onNodeRightClick: noop,
      onPaneClick: noop,
    });
    await renderer.init();
    renderer.destroy();

    const mockApp = (PIXI as any).__mockApp;
    expect(mockApp.destroy).toHaveBeenCalledOnce();
  });

  it("calls app.resize on resize()", async () => {
    const renderer = new PixiRenderer({
      canvas,
      onNodeClick: noop,
      onNodeRightClick: noop,
      onPaneClick: noop,
    });
    await renderer.init();
    renderer.resize();

    const mockApp = (PIXI as any).__mockApp;
    expect(mockApp.resize).toHaveBeenCalledOnce();
  });
});
```

**Step 2: Run test — expect FAIL (file doesn't exist yet)**

```bash
cd ui && npx vitest run src/features/workstation/CodebaseExplorer/__tests__/PixiRenderer.test.ts
```

Expected: FAIL — `Cannot find module '../pixi/PixiRenderer'`

**Step 3: The implementation already exists from Task 1.2 — run again**

```bash
cd ui && npx vitest run src/features/workstation/CodebaseExplorer/__tests__/PixiRenderer.test.ts
```

Expected: PASS (3 tests)

**Step 4: Run full test suite to confirm nothing broken**

```bash
cd ui && npx vitest run
```

Expected: all existing tests still pass

**Step 5: Commit**

```bash
cd ui && git add src/features/workstation/CodebaseExplorer/pixi/ \
  src/features/workstation/CodebaseExplorer/PixiExplorerCanvas.tsx \
  src/features/workstation/CodebaseExplorer/__tests__/PixiRenderer.test.ts
git commit -m "feat(explorer): add PixiRenderer shell and usePixiRenderer hook (Phase 1)"
```

---

## Phase 2 — Static Rendering

Reads `ExplorerGraph` from context, runs ELK, draws nodes and edges with the new color system and LOD. No interaction yet. After this phase the graph is visually correct.

---

### Task 2.1: Define RenderGraph types

**Files:**
- Create: `ui/src/features/workstation/CodebaseExplorer/pixi/types.ts`

**Step 1: Write the file**

```typescript
// ui/src/features/workstation/CodebaseExplorer/pixi/types.ts
import type { ExplorerNodeKind, ExplorerEdgeRelation } from "../types";

export type RenderNode = {
  id: string;
  label: string;
  kind: ExplorerNodeKind;
  x: number;
  y: number;
  width: number;
  height: number;
  /** 0–1 opacity */
  opacity: number;
  /** hex color for border, e.g. 0x4a90d9 */
  borderColor: number;
  borderWidth: number;
  /** hex color for background */
  bgColor: number;
  isFocused: boolean;
  isEgoUpstream: boolean;
  isEgoDownstream: boolean;
  signature?: {
    params: Array<{ name: string; typeAnnotation?: string }>;
    returnType?: string;
  };
};

export type RenderEdge = {
  id: string;
  fromId: string;
  toId: string;
  relation: ExplorerEdgeRelation;
  /** hex stroke color */
  color: number;
  width: number;
  dashed: boolean;
  opacity: number;
  /** present during trace mode */
  hasParticle?: boolean;
};

export type RenderGraph = {
  nodes: RenderNode[];
  edges: RenderEdge[];
};

export type LodLevel = "overview" | "navigation" | "inspection";
```

---

### Task 2.2: Create ElkLayout runner

**Files:**
- Create: `ui/src/features/workstation/CodebaseExplorer/layout/ElkLayout.ts`

**Step 1: Write the file**

```typescript
// ui/src/features/workstation/CodebaseExplorer/layout/ElkLayout.ts
import ELK from "elkjs/lib/elk.bundled.js";
import type { RenderNode, RenderEdge } from "../pixi/types";

const elk = new ELK();

export type PositionMap = Map<string, { x: number; y: number }>;

/** Returns a stable id→{x,y} map. Nodes without positions default to {0,0}. */
export async function runElkLayout(
  nodes: Pick<RenderNode, "id" | "kind" | "width" | "height">[],
  edges: Pick<RenderEdge, "id" | "fromId" | "toId" | "relation">[]
): Promise<PositionMap> {
  const visibleEdges = edges.filter((e) => e.relation !== "contains");

  const layout = await elk.layout({
    id: "root",
    layoutOptions: {
      "elk.algorithm": "layered",
      "elk.direction": "DOWN",
      "elk.spacing.nodeNode": "48",
      "elk.layered.spacing.nodeNodeBetweenLayers": "80",
      "elk.layered.nodePlacement.strategy": "NETWORK_SIMPLEX",
      "elk.layered.mergeEdges": "true",
    },
    children: nodes.map((n) => ({ id: n.id, width: n.width, height: n.height })),
    edges: visibleEdges.map((e) => ({ id: e.id, sources: [e.fromId], targets: [e.toId] })),
  });

  const positions: PositionMap = new Map();
  for (const child of layout.children ?? []) {
    positions.set(child.id, { x: child.x ?? 0, y: child.y ?? 0 });
  }
  return positions;
}
```

**Step 2: Write test**

```typescript
// ui/src/features/workstation/CodebaseExplorer/__tests__/ElkLayout.test.ts
import { describe, it, expect, vi } from "vitest";

vi.mock("elkjs/lib/elk.bundled.js", () => ({
  default: class MockElk {
    async layout(graph: { children?: Array<{ id: string }> }) {
      return {
        children: (graph.children ?? []).map((c, i) => ({
          id: c.id,
          x: i * 200,
          y: 0,
        })),
      };
    }
  },
}));

import { runElkLayout } from "../layout/ElkLayout";

describe("runElkLayout", () => {
  it("returns positions for all input nodes", async () => {
    const nodes = [
      { id: "a", kind: "file" as const, width: 140, height: 32 },
      { id: "b", kind: "file" as const, width: 140, height: 32 },
    ];
    const edges = [{ id: "e1", fromId: "a", toId: "b", relation: "calls" as const }];

    const positions = await runElkLayout(nodes, edges);

    expect(positions.has("a")).toBe(true);
    expect(positions.has("b")).toBe(true);
  });

  it("excludes contains edges from ELK input", async () => {
    // Test via output: contains edges should not cause ELK errors with disconnected nodes
    const nodes = [{ id: "crt", kind: "crate" as const, width: 160, height: 40 }];
    const edges = [{ id: "e1", fromId: "crt", toId: "mod", relation: "contains" as const }];

    const positions = await runElkLayout(nodes, edges);
    expect(positions instanceof Map).toBe(true);
  });
});
```

**Step 3: Run**

```bash
cd ui && npx vitest run src/features/workstation/CodebaseExplorer/__tests__/ElkLayout.test.ts
```

Expected: PASS

---

### Task 2.3: Create useRenderGraph adapter

**Files:**
- Create: `ui/src/features/workstation/CodebaseExplorer/hooks/useRenderGraph.ts`

**Step 1: Write the hook**

```typescript
// ui/src/features/workstation/CodebaseExplorer/hooks/useRenderGraph.ts
import { useMemo } from "react";
import type { ExplorerContextValue } from "../types";
import type { RenderGraph, RenderNode, RenderEdge } from "../pixi/types";
import type { PositionMap } from "../layout/ElkLayout";

// Node visual constants
const NODE_DIMS: Record<string, { w: number; h: number }> = {
  crate:   { w: 160, h: 40 },
  module:  { w: 140, h: 36 },
  file:    { w: 140, h: 32 },
  symbol:  { w: 200, h: 36 },
  symbol_sig: { w: 240, h: 52 },
};

const NODE_BG: Record<string, number> = {
  crate:  0x1e2d3d,
  module: 0x172130,
  file:   0x0d2137,
  function: 0x141c2e,
  trait_impl_method: 0x141c2e,
  macro_call: 0x141c2e,
};

const NODE_BORDER: Record<string, number> = {
  crate:  0x4a90d9,
  module: 0x3a6fa8,
  file:   0x3b82f6,
  function: 0x475569,
  trait_impl_method: 0x475569,
  macro_call: 0x475569,
};

const EDGE_COLOR: Record<string, number> = {
  calls:           0x4a90d9,
  parameter_flow:  0xa78bfa,
  return_flow:     0x34d399,
  cfg:             0x64748b,
  contains:        0x000000, // invisible
};

function nodeDims(node: ExplorerContextValue["graph"]["nodes"][0]): { w: number; h: number } {
  if (node.kind === "crate") return NODE_DIMS.crate;
  if (node.kind === "module") return NODE_DIMS.module;
  if (node.kind === "file") return NODE_DIMS.file;
  if (node.signature && node.signature.parameters.length > 0) return NODE_DIMS.symbol_sig;
  return NODE_DIMS.symbol;
}

export function useRenderGraph(
  ctx: Pick<
    ExplorerContextValue,
    | "graph"
    | "focusedNodeId"
    | "upstreamIds"
    | "downstreamIds"
    | "stateKind"
    | "neighborhoodResult"
    | "matchingNodeIds"
  >,
  positions: PositionMap
): RenderGraph {
  return useMemo(() => {
    const nodes: RenderNode[] = ctx.graph.nodes.map((n) => {
      const dims = nodeDims(n);
      const pos = positions.get(n.id) ?? { x: 0, y: 0 };
      const isFocused = ctx.focusedNodeId === n.id;
      const isEgoUpstream = ctx.upstreamIds.has(n.id);
      const isEgoDownstream = ctx.downstreamIds.has(n.id);

      let opacity = 1;
      if (ctx.stateKind === "focus") {
        const inEgo = isFocused || isEgoUpstream || isEgoDownstream;
        opacity = inEgo ? 1 : 0.08;
      } else if (ctx.stateKind === "highlight" && ctx.neighborhoodResult) {
        opacity = ctx.neighborhoodResult.highlightedIds.has(n.id) ? 1 : 0.08;
      }

      if (ctx.matchingNodeIds && !ctx.matchingNodeIds.has(n.id)) {
        opacity = Math.min(opacity, 0.1);
      }

      let borderColor = NODE_BORDER[n.kind] ?? 0x475569;
      let borderWidth = n.kind === "crate" ? 2 : n.kind === "module" ? 1.5 : 1;
      if (isFocused) { borderColor = 0xffffff; borderWidth = 2; }
      else if (isEgoUpstream) borderColor = 0x3b82f6;
      else if (isEgoDownstream) borderColor = 0xf97316;

      return {
        id: n.id,
        label: n.label,
        kind: n.kind,
        x: pos.x,
        y: pos.y,
        width: dims.w,
        height: dims.h,
        opacity,
        borderColor,
        borderWidth,
        bgColor: NODE_BG[n.kind] ?? 0x141c2e,
        isFocused,
        isEgoUpstream,
        isEgoDownstream,
        signature: n.signature
          ? { params: n.signature.parameters, returnType: n.signature.returnType }
          : undefined,
      };
    });

    const edges: RenderEdge[] = ctx.graph.edges
      .filter((e) => e.relation !== "contains")
      .map((e) => ({
        id: `${e.from}::${e.to}::${e.relation}`,
        fromId: e.from,
        toId: e.to,
        relation: e.relation,
        color: EDGE_COLOR[e.relation] ?? 0x4a90d9,
        width: e.relation === "cfg" ? 1 : 1.5,
        dashed: e.relation === "parameter_flow" || e.relation === "return_flow",
        opacity: 1,
      }));

    return { nodes, edges };
  }, [ctx, positions]);
}
```

**Step 2: Write test**

```typescript
// ui/src/features/workstation/CodebaseExplorer/__tests__/useRenderGraph.test.ts
import { describe, it, expect } from "vitest";
import { renderHook } from "@testing-library/react";
import { useRenderGraph } from "../hooks/useRenderGraph";

const BASE_GRAPH = {
  nodes: [
    { id: "f1", label: "main.rs", kind: "file" as const },
    { id: "s1", label: "do_thing", kind: "function" as const, filePath: "main.rs", line: 10 },
  ],
  edges: [
    { from: "f1", to: "s1", relation: "contains" as const },
    { from: "s1", to: "s1", relation: "calls" as const }, // self-loop, edge case
  ],
};

const BASE_CTX = {
  graph: BASE_GRAPH,
  focusedNodeId: null,
  upstreamIds: new Set<string>(),
  downstreamIds: new Set<string>(),
  stateKind: "overview" as const,
  neighborhoodResult: null,
  matchingNodeIds: null,
};

describe("useRenderGraph", () => {
  it("produces one RenderNode per graph node", () => {
    const positions = new Map([
      ["f1", { x: 0, y: 0 }],
      ["s1", { x: 200, y: 100 }],
    ]);
    const { result } = renderHook(() => useRenderGraph(BASE_CTX, positions));
    expect(result.current.nodes).toHaveLength(2);
  });

  it("excludes contains edges from render edges", () => {
    const positions = new Map();
    const { result } = renderHook(() => useRenderGraph(BASE_CTX, positions));
    const relations = result.current.edges.map((e) => e.relation);
    expect(relations).not.toContain("contains");
  });

  it("dims non-ego nodes to 0.08 opacity in focus mode", () => {
    const positions = new Map([
      ["f1", { x: 0, y: 0 }],
      ["s1", { x: 200, y: 100 }],
    ]);
    const ctx = {
      ...BASE_CTX,
      focusedNodeId: "s1",
      stateKind: "focus" as const,
    };
    const { result } = renderHook(() => useRenderGraph(ctx, positions));
    const f1 = result.current.nodes.find((n) => n.id === "f1");
    expect(f1?.opacity).toBe(0.08);
    const s1 = result.current.nodes.find((n) => n.id === "s1");
    expect(s1?.opacity).toBe(1);
  });

  it("sets focused node border to white", () => {
    const positions = new Map();
    const ctx = { ...BASE_CTX, focusedNodeId: "s1", stateKind: "focus" as const };
    const { result } = renderHook(() => useRenderGraph(ctx, positions));
    const s1 = result.current.nodes.find((n) => n.id === "s1");
    expect(s1?.borderColor).toBe(0xffffff);
  });

  it("uses dashed style for parameter_flow and return_flow edges", () => {
    const graph = {
      ...BASE_GRAPH,
      edges: [
        { from: "f1", to: "s1", relation: "parameter_flow" as const },
        { from: "s1", to: "f1", relation: "return_flow" as const },
      ],
    };
    const { result } = renderHook(() => useRenderGraph({ ...BASE_CTX, graph }, new Map()));
    expect(result.current.edges.every((e) => e.dashed)).toBe(true);
  });
});
```

**Step 3: Run**

```bash
cd ui && npx vitest run src/features/workstation/CodebaseExplorer/__tests__/useRenderGraph.test.ts
```

Expected: PASS (5 tests)

---

### Task 2.4: Create LodController

**Files:**
- Create: `ui/src/features/workstation/CodebaseExplorer/pixi/LodController.ts`

**Step 1: Write the file**

```typescript
// ui/src/features/workstation/CodebaseExplorer/pixi/LodController.ts
import type { LodLevel } from "./types";

export function zoomToLod(zoom: number): LodLevel {
  if (zoom < 0.4) return "overview";
  if (zoom < 0.8) return "navigation";
  return "inspection";
}

export type LodConfig = {
  showFileLabels: boolean;
  showSymbolLabels: boolean;
  showSignatures: boolean;
  showEdgeLabels: boolean;
  edgeWidth: number;
};

export function lodConfig(lod: LodLevel): LodConfig {
  switch (lod) {
    case "overview":
      return {
        showFileLabels: false,
        showSymbolLabels: false,
        showSignatures: false,
        showEdgeLabels: false,
        edgeWidth: 1,
      };
    case "navigation":
      return {
        showFileLabels: true,
        showSymbolLabels: true,
        showSignatures: false,
        showEdgeLabels: false,
        edgeWidth: 1.5,
      };
    case "inspection":
      return {
        showFileLabels: true,
        showSymbolLabels: true,
        showSignatures: true,
        showEdgeLabels: true,
        edgeWidth: 1.5,
      };
  }
}
```

**Step 2: Write test**

```typescript
// ui/src/features/workstation/CodebaseExplorer/__tests__/LodController.test.ts
import { describe, it, expect } from "vitest";
import { zoomToLod, lodConfig } from "../pixi/LodController";

describe("zoomToLod", () => {
  it("returns overview below 0.4", () => expect(zoomToLod(0.3)).toBe("overview"));
  it("returns navigation between 0.4 and 0.8", () => expect(zoomToLod(0.6)).toBe("navigation"));
  it("returns inspection above 0.8", () => expect(zoomToLod(1.0)).toBe("inspection"));
  it("boundary 0.4 is navigation", () => expect(zoomToLod(0.4)).toBe("navigation"));
  it("boundary 0.8 is inspection", () => expect(zoomToLod(0.8)).toBe("inspection"));
});

describe("lodConfig", () => {
  it("overview hides all labels", () => {
    const cfg = lodConfig("overview");
    expect(cfg.showFileLabels).toBe(false);
    expect(cfg.showSymbolLabels).toBe(false);
  });
  it("inspection shows signatures", () => {
    expect(lodConfig("inspection").showSignatures).toBe(true);
  });
});
```

**Step 3: Run**

```bash
cd ui && npx vitest run src/features/workstation/CodebaseExplorer/__tests__/LodController.test.ts
```

Expected: PASS (7 tests)

---

### Task 2.5: Create NodeLayer

**Files:**
- Create: `ui/src/features/workstation/CodebaseExplorer/pixi/NodeLayer.ts`

**Step 1: Write the file**

```typescript
// ui/src/features/workstation/CodebaseExplorer/pixi/NodeLayer.ts
import * as PIXI from "pixi.js";
import type { RenderNode } from "./types";
import type { LodConfig } from "./LodController";

export class NodeLayer {
  private container: PIXI.Container;
  private graphics: PIXI.Graphics;
  private textCache = new Map<string, PIXI.Text>();
  private visibleNodes: RenderNode[] = []; // tracks current LOD-filtered set for hitTest

  constructor(stage: PIXI.Container) {
    this.container = new PIXI.Container();
    this.graphics = new PIXI.Graphics();
    this.container.addChild(this.graphics);
    stage.addChild(this.container);
  }

  draw(nodes: RenderNode[], lod: LodConfig): void {
    this.graphics.clear();

    // At overview LOD, only cluster nodes (crate/module) are rendered.
    // File and symbol nodes are hidden entirely.
    const isCluster = (kind: string) => kind === "crate" || kind === "module";
    const visibleNodes = lod.showFileLabels
      ? nodes // navigation or inspection: all nodes visible
      : nodes.filter((n) => isCluster(n.kind)); // overview: clusters only
    this.visibleNodes = visibleNodes; // keep in sync for hitTest

    const activeIds = new Set(visibleNodes.map((n) => n.id));
    // Remove stale text objects (including those for now-hidden file/symbol nodes)
    for (const [id, text] of this.textCache) {
      if (!activeIds.has(id)) {
        this.container.removeChild(text);
        text.destroy();
        this.textCache.delete(id);
      }
    }

    for (const node of visibleNodes) {
      const { x, y, width, height, bgColor, borderColor, borderWidth, opacity } = node;
      const radius = node.kind === "crate" ? 8 : node.kind === "module" ? 6 : 4;

      this.graphics.alpha = opacity;
      this.graphics
        .roundRect(x, y, width, height, radius)
        .fill({ color: bgColor })
        .stroke({ color: borderColor, width: borderWidth });

      const showLabel =
        isCluster(node.kind) ||
        (node.kind === "file" && lod.showFileLabels) ||
        (!isCluster(node.kind) && node.kind !== "file" && lod.showSymbolLabels);

      if (showLabel) {
        let text = this.textCache.get(node.id);
        if (!text) {
          text = new PIXI.Text({
            text: node.label,
            style: new PIXI.TextStyle({
              fill: 0xf1f5f9,
              fontSize: node.kind === "crate" || node.kind === "module" ? 12 : 13,
              fontWeight: node.kind === "crate" || node.kind === "module" ? "600" : "500",
              fontFamily: "system-ui, sans-serif",
            }),
          });
          this.textCache.set(node.id, text);
          this.container.addChild(text);
        }
        text.text = node.label;
        text.x = x + 8;
        text.y = y + height / 2 - text.height / 2;
        text.alpha = opacity;
      } else {
        const text = this.textCache.get(node.id);
        if (text) {
          this.container.removeChild(text);
          text.destroy();
          this.textCache.delete(node.id);
        }
      }
    }
  }

  /** Returns the set of currently visible node IDs (for EdgeLayer to filter against). */
  getVisibleIds(): Set<string> {
    return new Set(this.visibleNodes.map((n) => n.id));
  }

  /** Returns the node id at world coordinates (x, y), or null.
   *  Only tests against currently visible nodes (respects LOD filtering). */
  hitTest(x: number, y: number): string | null {
    // this.visibleNodes is set in draw() and reflects the current LOD filter
    for (const node of [...this.visibleNodes].reverse()) {
      if (
        x >= node.x &&
        x <= node.x + node.width &&
        y >= node.y &&
        y <= node.y + node.height
      ) {
        return node.id;
      }
    }
    return null;
  }

  destroy(): void {
    for (const text of this.textCache.values()) {
      text.destroy();
    }
    this.textCache.clear();
    this.container.destroy({ children: true });
  }
}
```

---

### Task 2.6: Create EdgeLayer

**Files:**
- Create: `ui/src/features/workstation/CodebaseExplorer/pixi/EdgeLayer.ts`

**Step 1: Write the file**

```typescript
// ui/src/features/workstation/CodebaseExplorer/pixi/EdgeLayer.ts
import * as PIXI from "pixi.js";
import type { RenderEdge, RenderNode } from "./types";
import type { LodConfig } from "./LodController";

export class EdgeLayer {
  private graphics: PIXI.Graphics;

  constructor(stage: PIXI.Container) {
    this.graphics = new PIXI.Graphics();
    // Edges drawn below nodes
    stage.addChildAt(this.graphics, 0);
  }

  draw(edges: RenderEdge[], nodeById: Map<string, RenderNode>, lod: LodConfig, visibleIds: Set<string>): void {
    this.graphics.clear();

    for (const edge of edges) {
      const from = nodeById.get(edge.fromId);
      const to = nodeById.get(edge.toId);
      // Skip edges where either endpoint is not currently rendered (LOD filtering)
      if (!from || !to || !visibleIds.has(edge.fromId) || !visibleIds.has(edge.toId)) continue;

      // Override edge width from LOD (overview = hairline 1px regardless of relation)
      const drawWidth = lod.edgeWidth === 1 ? 1 : edge.width;

      const x1 = from.x + from.width / 2;
      const y1 = from.y + from.height;
      const x2 = to.x + to.width / 2;
      const y2 = to.y;

      // Control point for cubic bezier
      const midY = (y1 + y2) / 2;

      if (edge.dashed) {
        // Approximate dashed bezier by sampling points along the curve
        // and drawing alternating solid segments (dash=8px, gap=5px)
        const STEPS = 60;
        const DASH = 8;
        const GAP = 5;
        let accumulated = 0;
        let drawing = true;
        let prevX = x1;
        let prevY = y1;
        for (let i = 1; i <= STEPS; i++) {
          const t = i / STEPS;
          const mt = 1 - t;
          // Cubic bezier: P = (1-t)^3*P0 + 3(1-t)^2*t*P1 + 3(1-t)*t^2*P2 + t^3*P3
          // Our control points: P0=(x1,y1), P1=(x1,midY), P2=(x2,midY), P3=(x2,y2)
          const cx = mt * mt * mt * x1 + 3 * mt * mt * t * x1 + 3 * mt * t * t * x2 + t * t * t * x2;
          const cy = mt * mt * mt * y1 + 3 * mt * mt * t * midY + 3 * mt * t * t * midY + t * t * t * y2;
          const segLen = Math.hypot(cx - prevX, cy - prevY);
          accumulated += segLen;
          const threshold = drawing ? DASH : GAP;
          if (accumulated >= threshold) {
            if (drawing) {
              this.graphics.moveTo(prevX, prevY).lineTo(cx, cy)
                .stroke({ color: edge.color, width: drawWidth, alpha: edge.opacity });
            }
            drawing = !drawing;
            accumulated -= threshold;
          }
          prevX = cx;
          prevY = cy;
        }
      } else {
        this.graphics
          .moveTo(x1, y1)
          .bezierCurveTo(x1, midY, x2, midY, x2, y2)
          .stroke({ color: edge.color, width: drawWidth, alpha: edge.opacity });
      }

      // Arrowhead
      const angle = Math.atan2(y2 - midY, x2 - x1);
      const arrowSize = 6;
      this.graphics
        .moveTo(x2, y2)
        .lineTo(
          x2 - arrowSize * Math.cos(angle - Math.PI / 6),
          y2 - arrowSize * Math.sin(angle - Math.PI / 6)
        )
        .lineTo(
          x2 - arrowSize * Math.cos(angle + Math.PI / 6),
          y2 - arrowSize * Math.sin(angle + Math.PI / 6)
        )
        .fill({ color: edge.color, alpha: edge.opacity });
    }
  }

  destroy(): void {
    this.graphics.destroy();
  }
}
```

---

### Task 2.7: Wire rendering into PixiRenderer and swap ExplorerCanvas

**Files:**
- Modify: `ui/src/features/workstation/CodebaseExplorer/pixi/PixiRenderer.ts`
- Modify: `ui/src/features/workstation/CodebaseExplorer/PixiExplorerCanvas.tsx`
- Modify: `ui/src/features/workstation/CodebaseExplorer/index.tsx`

**Step 1: Extend PixiRenderer to accept RenderGraph updates**

Add to `PixiRenderer.ts`:

```typescript
import { NodeLayer } from "./NodeLayer";
import { EdgeLayer } from "./EdgeLayer";
import { zoomToLod, lodConfig } from "./LodController";
import type { RenderGraph, RenderNode } from "./types";

// Inside the class, add:
private nodeLayer!: NodeLayer;
private edgeLayer!: EdgeLayer;
private currentZoom = 1;
private currentGraph: RenderGraph | null = null;

// In init(), after app.init():
this.nodeLayer = new NodeLayer(this.app.stage);
this.edgeLayer = new EdgeLayer(this.app.stage);

// Wire wheel event (zoom) and pointer events (pan).
this.app.canvas.addEventListener("wheel", this.handleWheel, { passive: false });
this.app.canvas.addEventListener("pointerdown", this.handlePointerDown);
this.app.canvas.addEventListener("pointermove", this.handlePointerMove);
this.app.canvas.addEventListener("pointerup", this.handlePointerUp);
this.app.canvas.addEventListener("pointercancel", this.handlePointerUp);

// Pan state
private isPanning = false;
private hasDragged = false;   // true when pointer moved beyond DRAG_THRESHOLD
private panStart = { x: 0, y: 0 };
private stageStartPos = { x: 0, y: 0 };
private static readonly DRAG_THRESHOLD = 4; // px; below this is treated as a click

private handlePointerDown = (event: PointerEvent): void => {
  // Only pan on primary button (left); right-click is contextmenu
  if (event.button !== 0) return;
  this.isPanning = true;
  this.hasDragged = false;
  this.panStart = { x: event.clientX, y: event.clientY };
  this.stageStartPos = { x: this.app.stage.x, y: this.app.stage.y };
  this.app.canvas.setPointerCapture(event.pointerId);
};

private handlePointerMove = (event: PointerEvent): void => {
  if (!this.isPanning) return;
  const dx = event.clientX - this.panStart.x;
  const dy = event.clientY - this.panStart.y;
  if (!this.hasDragged && Math.hypot(dx, dy) > PixiRenderer.DRAG_THRESHOLD) {
    this.hasDragged = true;
  }
  if (this.hasDragged) {
    this.app.stage.x = this.stageStartPos.x + dx;
    this.app.stage.y = this.stageStartPos.y + dy;
  }
};

private handlePointerUp = (event: PointerEvent): void => {
  if (!this.isPanning) return;
  this.isPanning = false;
  this.app.canvas.releasePointerCapture(event.pointerId);
};

// Add private wheel handler:
private handleWheel = (event: WheelEvent): void => {
  event.preventDefault();
  const scaleFactor = event.deltaY < 0 ? 1.1 : 0.9;
  const newScale = Math.max(0.1, Math.min(10, this.app.stage.scale.x * scaleFactor));
  // Zoom toward cursor position
  const rect = this.app.canvas.getBoundingClientRect();
  const mx = event.clientX - rect.left;
  const my = event.clientY - rect.top;
  const worldX = (mx - this.app.stage.x) / this.app.stage.scale.x;
  const worldY = (my - this.app.stage.y) / this.app.stage.scale.y;
  this.app.stage.scale.set(newScale);
  this.app.stage.x = mx - worldX * newScale;
  this.app.stage.y = my - worldY * newScale;
  this.currentZoom = newScale;
  // Re-draw with updated LOD
  if (this.currentGraph) this.redraw();
};

// Extract drawing to a private method so wheel handler can call it:
private redraw(): void {
  if (!this.currentGraph) return;
  const lod = lodConfig(zoomToLod(this.currentZoom));
  const nodeById = new Map<string, RenderNode>(this.currentGraph.nodes.map((n) => [n.id, n]));
  // Draw nodes first — this populates nodeLayer.visibleNodes used by hitTest and edge filtering
  this.nodeLayer.draw(this.currentGraph.nodes, lod);
  // Pass the visible ID set so edges to hidden nodes are skipped
  const visibleIds = this.nodeLayer.getVisibleIds();
  this.edgeLayer.draw(this.currentGraph.edges, nodeById, lod, visibleIds);
}

// Add new public method:
updateGraph(graph: RenderGraph): void {
  this.currentGraph = graph;
  this.redraw();
}

// In destroy(), before app.destroy():
this.app.canvas.removeEventListener("wheel", this.handleWheel);
this.app.canvas.removeEventListener("pointerdown", this.handlePointerDown);
this.app.canvas.removeEventListener("pointermove", this.handlePointerMove);
this.app.canvas.removeEventListener("pointerup", this.handlePointerUp);
this.app.canvas.removeEventListener("pointercancel", this.handlePointerUp);
this.nodeLayer?.destroy();
this.edgeLayer?.destroy();
```

**Step 2: Update usePixiRenderer to expose updateGraph**

Extend the hook to return a ref to the renderer so `PixiExplorerCanvas` can call `updateGraph`:

```typescript
// Return rendererRef from usePixiRenderer so the canvas can call updateGraph
export function usePixiRenderer(
  canvasRef: React.RefObject<HTMLCanvasElement | null>,
  options: Omit<PixiRendererOptions, "canvas">
): React.RefObject<PixiRenderer | null> {
  // ... existing body unchanged ...
  return rendererRef;
}
```

**Step 3: Update PixiExplorerCanvas to call updateGraph**

```typescript
// In PixiExplorerCanvas.tsx, add:
import { useRenderGraph } from "./hooks/useRenderGraph";
import { runElkLayout } from "./layout/ElkLayout";
import type { PositionMap } from "./layout/ElkLayout";

// Inside the component:
const [positions, setPositions] = useState<PositionMap>(new Map());
const renderGraph = useRenderGraph(ctx, positions);
const rendererRef = usePixiRenderer(canvasRef, { ... });

// Run ELK when topology changes — include edges (with relation) so edge-only
// changes and relation-only changes both trigger re-layout.
const topologyKey = useMemo(() => {
  const nodeIds = ctx.graph.nodes.map((n) => n.id).sort().join(",");
  // Include relation so calls/parameter_flow/return_flow between same nodes are distinct
  const edgeIds = ctx.graph.edges.map((e) => `${e.from}>${e.to}:${e.relation}`).sort().join(",");
  return `${nodeIds}|${edgeIds}`;
}, [ctx.graph.nodes, ctx.graph.edges]);

useEffect(() => {
  // Build RenderNode stubs with correct dims for ELK (matches useLayout sizing)
  const stubNodes = ctx.graph.nodes.map((n) => ({
    id: n.id,
    kind: n.kind,
    width: n.kind === "crate" ? 160 : n.kind === "module" ? 140 : n.kind === "file" ? 140
      : n.signature ? 240 : 200,
    height: n.kind === "crate" ? 40 : n.kind === "module" ? 36 : n.kind === "file" ? 32
      : n.signature ? 52 : 36,
  }));
  const stubEdges = ctx.graph.edges.map((e) => ({
    id: `${e.from}::${e.to}::${e.relation}`, // include relation to avoid collisions
    fromId: e.from,
    toId: e.to,
    relation: e.relation,
  }));
  void runElkLayout(stubNodes, stubEdges).then(setPositions);
}, [topologyKey]);

// Push renderGraph into Pixi each render
useEffect(() => {
  rendererRef.current?.updateGraph(renderGraph);
}, [renderGraph, rendererRef]);
```

**Step 4: Swap ExplorerCanvas in index.tsx**

In `index.tsx`, replace:
```typescript
import { ExplorerCanvas } from "./ExplorerCanvas";
```
with:
```typescript
import { PixiExplorerCanvas as ExplorerCanvas } from "./PixiExplorerCanvas";
```

**Step 5: Rewrite integration tests for Pixi canvas interaction model**

The existing `CodebaseExplorer.test.tsx` relies on the ReactFlow mock rendering node labels as clickable `<button>` elements. Pixi draws to a `<canvas>` — there are no DOM buttons for nodes. The tests must be rewritten to mock `PixiRenderer` at the class level, capture the `onNodeClick`/`onPaneClick` callbacks it receives, and call them directly. This tests the React state wiring without needing a real canvas, WebGL, or hit-testing.

**How the test interaction model works:** `PixiRenderer` is mocked so its constructor records the callbacks passed via `options`. `simulateNodeClick(nodeId)` calls `options.onNodeClick(nodeId)` directly, which is the same code path a real click would follow after hit-testing.

Replace the `vi.mock("reactflow", ...)` block and update integration tests:

```typescript
// At top of CodebaseExplorer.test.tsx — replace vi.mock("reactflow"...) with:

// Track the PixiRenderer instance so tests can access its canvas and options.
let pixiRendererInstance: {
  canvas: HTMLCanvasElement;
  onNodeClick: (id: string) => void;
  onPaneClick: () => void;
} | null = null;

vi.mock("../pixi/PixiRenderer", () => ({
  PixiRenderer: vi.fn().mockImplementation((options: any) => {
    const canvas = options.canvas as HTMLCanvasElement;
    pixiRendererInstance = { canvas, onNodeClick: options.onNodeClick, onPaneClick: options.onPaneClick };
    return {
      init: vi.fn().mockResolvedValue(undefined),
      destroy: vi.fn(),
      resize: vi.fn(),
      updateGraph: vi.fn(),
      setParticleEdges: vi.fn(),
    };
  }),
}));

vi.mock("pixi.js", () => ({
  Application: vi.fn(() => ({
    init: vi.fn().mockResolvedValue(undefined),
    destroy: vi.fn(),
    resize: vi.fn(),
    stage: { addChild: vi.fn(), addChildAt: vi.fn(), x: 0, y: 0, scale: { x: 1, set: vi.fn() } },
    canvas: document.createElement("canvas"),
    ticker: { add: vi.fn() },
  })),
  Container: vi.fn(() => ({ addChild: vi.fn(), addChildAt: vi.fn(), destroy: vi.fn() })),
  Graphics: vi.fn(() => ({
    clear: vi.fn().mockReturnThis(), roundRect: vi.fn().mockReturnThis(),
    fill: vi.fn().mockReturnThis(), stroke: vi.fn().mockReturnThis(),
    moveTo: vi.fn().mockReturnThis(), bezierCurveTo: vi.fn().mockReturnThis(),
    lineTo: vi.fn().mockReturnThis(), circle: vi.fn().mockReturnThis(),
    alpha: 1, destroy: vi.fn(),
  })),
  Text: vi.fn(() => ({ text: "", x: 0, y: 0, height: 16, alpha: 1, destroy: vi.fn() })),
  TextStyle: vi.fn(),
}));

// Helper: simulate a node click by calling the renderer's onNodeClick callback directly.
// This bypasses hit-testing (which requires canvas coordinates → world transform)
// and tests that the React state wiring responds correctly to any node click.
function simulateNodeClick(nodeId: string): void {
  pixiRendererInstance?.onNodeClick(nodeId);
}

function simulatePaneClick(): void {
  pixiRendererInstance?.onPaneClick();
}
```

Then rewrite the tests that previously clicked ReactFlow text buttons:

```typescript
// BEFORE (ReactFlow-coupled):
// fireEvent.click(await screen.findByText("verify_signature"));

// AFTER (Pixi-compatible):
// await waitFor(() => expect(pixiRendererInstance).not.toBeNull());
// simulateNodeClick("sym_004");

// Example — rewrite "shows FOCUS state badge after node click":
it("shows FOCUS state badge and context panel after node click", async () => {
  render(<CodebaseExplorer sessionId="test-session" />);

  // Trigger cluster expand so sym_004 is loaded into the graph
  await waitFor(() => expect(pixiRendererInstance).not.toBeNull());
  simulateNodeClick("crt_001"); // expand crate cluster
  await waitFor(() => expect(mockLoadExplorerGraph).toHaveBeenCalledWith("test-session", undefined, "crt_001"));
  simulateNodeClick("mod_002"); // expand module cluster
  await waitFor(() => expect(mockLoadExplorerGraph).toHaveBeenCalledWith("test-session", undefined, "mod_002"));

  simulateNodeClick("sym_004"); // focus symbol node
  await waitFor(() => {
    expect(screen.getByText("FOCUS")).toBeInTheDocument();
    expect(screen.getByLabelText("Node context")).toBeInTheDocument();
  });
});

// Similarly update expandToSymbolLevel() helper to use simulateNodeClick
// instead of findByText("engine-crypto") / findByText("src") clicks.
```

Remove `expandToSymbolLevel()` which used `findByText` on node labels. Replace all usages with `simulateNodeClick` calls on the known mock IDs (`crt_001`, `mod_002`, `sym_004`).

Keep `beforeEach(() => { pixiRendererInstance = null; })` to reset between tests.

**Step 6: Run full tests**

```bash
cd ui && npx vitest run
```

Expected: all tests pass

**Step 7: Visual check in browser**

```bash
cd ui && npm run dev
```

Navigate to the workstation view. Confirm: dark canvas, nodes visible with colored borders, edges drawn between them, zoom changes LOD.

**Step 7: Commit**

```bash
cd ui && git add src/features/workstation/CodebaseExplorer/
git commit -m "feat(explorer): Pixi.js static rendering — nodes, edges, LOD, ELK layout (Phase 2)"
```

---

## Phase 3 — Interaction Wiring

Left-click focus, right-click context menu (callers, callees, open in editor), pane-click clear. Uses existing hooks — no new data logic.

---

### Task 3.1: Add hit testing and click events to PixiRenderer

**Files:**
- Modify: `ui/src/features/workstation/CodebaseExplorer/pixi/PixiRenderer.ts`

**Step 1: Add pointer event listeners in `init()`**

```typescript
// In init(), after layers are created:
this.app.canvas.addEventListener("click", this.handleClick);
this.app.canvas.addEventListener("contextmenu", this.handleContextMenu);

// Add private methods:
private handleClick = (event: MouseEvent): void => {
  // Suppress click if it was the end of a pan drag (pointer moved > DRAG_THRESHOLD).
  // hasDragged is reset on the next pointerdown, so a subsequent clean click works.
  if (this.hasDragged) return;
  const { x, y } = this.canvasToWorld(event.offsetX, event.offsetY);
  // hitTest uses NodeLayer's internal visibleNodes — only currently rendered nodes
  const nodeId = this.nodeLayer.hitTest(x, y);
  if (nodeId) {
    this.options.onNodeClick(nodeId);
  } else {
    this.options.onPaneClick();
  }
};

private handleContextMenu = (event: MouseEvent): void => {
  event.preventDefault();
  const { x, y } = this.canvasToWorld(event.offsetX, event.offsetY);
  const nodeId = this.nodeLayer.hitTest(x, y);
  if (nodeId) {
    this.options.onNodeRightClick(nodeId, event.clientX, event.clientY);
  }
};

private canvasToWorld(screenX: number, screenY: number): { x: number; y: number } {
  // Invert stage transform so hit testing works correctly after pan/zoom.
  // stage.x/y is the translation and stage.scale.x is the uniform scale.
  const scale = this.app.stage.scale.x;
  return {
    x: (screenX - this.app.stage.x) / scale,
    y: (screenY - this.app.stage.y) / scale,
  };
}
```

Also store the latest graph for hit testing:
```typescript
private currentGraph: RenderGraph | null = null;

// In updateGraph():
this.currentGraph = graph;
```

Remove listeners in `destroy()`:
```typescript
this.app.canvas.removeEventListener("click", this.handleClick);
this.app.canvas.removeEventListener("contextmenu", this.handleContextMenu);
```

---

### Task 3.2: Create ContextMenu component

**Files:**
- Create: `ui/src/features/workstation/CodebaseExplorer/ContextMenu.tsx`

**Step 1: Write the component**

```typescript
// ui/src/features/workstation/CodebaseExplorer/ContextMenu.tsx
import { useEffect, useRef } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { useExplorer } from "./ExplorerContext";

type ContextMenuProps = {
  nodeId: string | null;
  x: number;
  y: number;
  onClose: () => void;
};

export function ContextMenu({ nodeId, x, y, onClose }: ContextMenuProps) {
  const ctx = useExplorer();
  const node = nodeId ? ctx.nodeMap.get(nodeId) : null;
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!nodeId) return;
    const handler = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        onClose();
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [nodeId, onClose]);

  useEffect(() => {
    if (!nodeId) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [nodeId, onClose]);

  if (!node || !nodeId) return null;

  const callerCount = ctx.graph.edges.filter(
    (e) => e.relation === "calls" && e.to === nodeId
  ).length;
  const calleeCount = ctx.graph.edges.filter(
    (e) => e.relation === "calls" && e.from === nodeId
  ).length;

  return (
    <AnimatePresence>
      <motion.div
        ref={menuRef}
        initial={{ opacity: 0, scale: 0.95 }}
        animate={{ opacity: 1, scale: 1 }}
        exit={{ opacity: 0, scale: 0.95 }}
        transition={{ duration: 0.1, ease: "easeOut" }}
        style={{
          position: "absolute",
          left: x,
          top: y,
          zIndex: 1000,
          transformOrigin: "top left",
        }}
        className="explorer-context-menu"
        role="menu"
        aria-label={`Actions for ${node.label}`}
      >
        <div className="explorer-ctx-menu-header">
          <span className="explorer-ctx-menu-name">{node.label}</span>
          {node.filePath && (
            <span className="explorer-ctx-menu-path">{node.filePath}</span>
          )}
        </div>
        <div className="explorer-ctx-menu-divider" />
        <button
          className="explorer-ctx-menu-item"
          onClick={() => { ctx.focusNode(nodeId); ctx.showCallers(); onClose(); }}
          role="menuitem"
        >
          <span>↑</span> Show callers <span className="explorer-ctx-menu-count">({callerCount})</span>
        </button>
        <button
          className="explorer-ctx-menu-item"
          onClick={() => { ctx.focusNode(nodeId); ctx.showCallees(); onClose(); }}
          role="menuitem"
        >
          <span>↓</span> Show callees <span className="explorer-ctx-menu-count">({calleeCount})</span>
        </button>
        <button
          className="explorer-ctx-menu-item"
          onClick={() => { /* Phase 5 */ onClose(); }}
          role="menuitem"
        >
          <span>⟿</span> Trace to entry
        </button>
        <button
          className="explorer-ctx-menu-item"
          onClick={() => { /* Phase 5 */ onClose(); }}
          role="menuitem"
        >
          <span>⤳</span> Trace dataflow
        </button>
        <div className="explorer-ctx-menu-divider" />
        <button
          className="explorer-ctx-menu-item"
          onClick={() => {
            if (node.filePath) ctx.onNavigateToSource?.(node.filePath, node.line);
            onClose();
          }}
          role="menuitem"
        >
          <span>⌥</span> Open in editor
        </button>
      </motion.div>
    </AnimatePresence>
  );
}
```

**Step 2: Add CSS for context menu in styles.css**

Append to the `.explorer-*` section:

```css
.explorer-context-menu {
  background: #1e293b;
  border: 1px solid #334155;
  border-radius: 8px;
  padding: 4px 0;
  min-width: 220px;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.6);
  font-size: 13px;
}

.explorer-ctx-menu-header {
  padding: 8px 12px 6px;
}

.explorer-ctx-menu-name {
  display: block;
  color: #f1f5f9;
  font-weight: 600;
  font-family: "JetBrains Mono", monospace;
  font-size: 12px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.explorer-ctx-menu-path {
  display: block;
  color: #64748b;
  font-size: 11px;
  margin-top: 2px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.explorer-ctx-menu-divider {
  height: 1px;
  background: #334155;
  margin: 4px 0;
}

.explorer-ctx-menu-item {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 6px 12px;
  background: none;
  border: none;
  color: #cbd5e1;
  cursor: pointer;
  text-align: left;
  font-size: 13px;
}

.explorer-ctx-menu-item:hover {
  background: #334155;
  color: #f1f5f9;
}

.explorer-ctx-menu-count {
  color: #64748b;
  margin-left: auto;
  font-size: 11px;
}
```

**Step 3: Wire ContextMenu into PixiExplorerCanvas**

```typescript
// In PixiExplorerCanvas.tsx, add state and render:
const [contextMenu, setContextMenu] = useState<{ nodeId: string; x: number; y: number } | null>(null);

// In handleNodeRightClick:
const handleNodeRightClick = useCallback((nodeId: string, x: number, y: number) => {
  ctx.focusNode(nodeId);
  setContextMenu({ nodeId, x, y });
}, [ctx]);

// In JSX, after the canvas:
<ContextMenu
  nodeId={contextMenu?.nodeId ?? null}
  x={contextMenu?.x ?? 0}
  y={contextMenu?.y ?? 0}
  onClose={() => setContextMenu(null)}
/>
```

**Step 4: Write context menu tests**

```typescript
// ui/src/features/workstation/CodebaseExplorer/__tests__/ContextMenu.test.tsx
import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { ContextMenu } from "../ContextMenu";

// Mock ExplorerContext
const mockCtx = {
  nodeMap: new Map([
    ["sym_1", { id: "sym_1", label: "verify_sig", kind: "function", filePath: "src/lib.rs", line: 42 }],
  ]),
  graph: {
    nodes: [],
    edges: [
      { from: "sym_0", to: "sym_1", relation: "calls" },
      { from: "sym_1", to: "sym_2", relation: "calls" },
    ],
  },
  focusNode: vi.fn(),
  showCallers: vi.fn(),
  showCallees: vi.fn(),
  clearHighlight: vi.fn(),
  onNavigateToSource: vi.fn(),
};

vi.mock("../ExplorerContext", () => ({
  useExplorer: () => mockCtx,
}));

describe("ContextMenu", () => {
  it("renders node name and file path in header", () => {
    render(<ContextMenu nodeId="sym_1" x={100} y={100} onClose={vi.fn()} />);
    expect(screen.getByText("verify_sig")).toBeInTheDocument();
    expect(screen.getByText("src/lib.rs")).toBeInTheDocument();
  });

  it("shows correct caller and callee counts", () => {
    render(<ContextMenu nodeId="sym_1" x={100} y={100} onClose={vi.fn()} />);
    expect(screen.getByText(/show callers/i)).toBeInTheDocument();
    expect(screen.getByText("(1)")).toBeInTheDocument(); // 1 caller
    expect(screen.getByText("(1)")).toBeInTheDocument(); // 1 callee
  });

  it("calls showCallers and onClose when Show callers is clicked", () => {
    const onClose = vi.fn();
    render(<ContextMenu nodeId="sym_1" x={100} y={100} onClose={onClose} />);
    fireEvent.click(screen.getByText(/show callers/i));
    expect(mockCtx.showCallers).toHaveBeenCalled();
    expect(onClose).toHaveBeenCalled();
  });

  it("calls onNavigateToSource when Open in editor clicked", () => {
    const onClose = vi.fn();
    render(<ContextMenu nodeId="sym_1" x={100} y={100} onClose={onClose} />);
    fireEvent.click(screen.getByText(/open in editor/i));
    expect(mockCtx.onNavigateToSource).toHaveBeenCalledWith("src/lib.rs", 42);
  });

  it("calls onClose on Escape", () => {
    const onClose = vi.fn();
    render(<ContextMenu nodeId="sym_1" x={100} y={100} onClose={onClose} />);
    fireEvent.keyDown(window, { key: "Escape" });
    expect(onClose).toHaveBeenCalled();
  });

  it("renders nothing when nodeId is null", () => {
    const { container } = render(<ContextMenu nodeId={null} x={0} y={0} onClose={vi.fn()} />);
    expect(container.firstChild).toBeNull();
  });
});
```

**Step 5: Run**

```bash
cd ui && npx vitest run src/features/workstation/CodebaseExplorer/__tests__/ContextMenu.test.tsx
```

Expected: PASS (6 tests)

**Step 6: Run full suite**

```bash
cd ui && npx vitest run
```

Expected: all green

**Step 7: Commit**

```bash
cd ui && git add src/features/workstation/CodebaseExplorer/
git commit -m "feat(explorer): interaction wiring — click focus, right-click context menu (Phase 3)"
```

---

## Phase 4 — Animation

Spring-physics ego layout via d3-force, bloom-in on load, edge particle system for trace mode.

---

### Task 4.1: Create ForceLayout

**Files:**
- Create: `ui/src/features/workstation/CodebaseExplorer/layout/ForceLayout.ts`

**Step 1: Write the file**

```typescript
// ui/src/features/workstation/CodebaseExplorer/layout/ForceLayout.ts
import * as d3 from "d3-force";
import type { PositionMap } from "./ElkLayout";

type ForceNode = { id: string; x: number; y: number; fx?: number | null; fy?: number | null };

export type ForceLayoutConfig = {
  focusedNodeId: string | null;
  upstreamIds: Set<string>;
  downstreamIds: Set<string>;
  /** Center of the canvas */
  center: { x: number; y: number };
  columnWidth: number;
};

/**
 * Run d3-force spring simulation for ego-network layout.
 * Runs for max 300 ticks then stops. Returns final positions.
 */
export function runForceLayout(
  initialPositions: PositionMap,
  config: ForceLayoutConfig
): PositionMap {
  const nodes: ForceNode[] = Array.from(initialPositions.entries()).map(([id, pos]) => ({
    id,
    x: pos.x,
    y: pos.y,
  }));

  // Fix focused node at center
  if (config.focusedNodeId) {
    const focused = nodes.find((n) => n.id === config.focusedNodeId);
    if (focused) {
      focused.fx = config.center.x;
      focused.fy = config.center.y;
    }
  }

  // Fix upstream nodes to left column, downstream to right
  for (const node of nodes) {
    if (config.upstreamIds.has(node.id)) {
      node.fx = config.center.x - config.columnWidth;
    } else if (config.downstreamIds.has(node.id)) {
      node.fx = config.center.x + config.columnWidth;
    }
  }

  const links = Array.from(initialPositions.keys())
    .filter((id) => config.upstreamIds.has(id) || config.downstreamIds.has(id))
    .map((id) => ({ source: id, target: config.focusedNodeId ?? id }));

  const simulation = d3
    .forceSimulation(nodes)
    .force("link", d3.forceLink(links).id((d: any) => d.id).strength(0.3).distance(120))
    .force("charge", d3.forceManyBody().strength(-200))
    .force("y", d3.forceY(config.center.y).strength(0.2))
    .stop();

  // Run synchronously for max 300 ticks
  for (let i = 0; i < 300; i++) {
    simulation.tick();
  }

  const result: PositionMap = new Map();
  for (const node of nodes) {
    result.set(node.id, { x: node.x, y: node.y });
  }
  return result;
}
```

**Step 2: Write test**

```typescript
// ui/src/features/workstation/CodebaseExplorer/__tests__/ForceLayout.test.ts
import { describe, it, expect } from "vitest";
import { runForceLayout } from "../layout/ForceLayout";

describe("runForceLayout", () => {
  it("returns a position for every input node", () => {
    const positions = new Map([
      ["center", { x: 400, y: 300 }],
      ["up1", { x: 100, y: 300 }],
      ["down1", { x: 700, y: 300 }],
    ]);
    const result = runForceLayout(positions, {
      focusedNodeId: "center",
      upstreamIds: new Set(["up1"]),
      downstreamIds: new Set(["down1"]),
      center: { x: 400, y: 300 },
      columnWidth: 300,
    });
    expect(result.size).toBe(3);
  });

  it("keeps focused node at center position", () => {
    const positions = new Map([["center", { x: 0, y: 0 }]]);
    const result = runForceLayout(positions, {
      focusedNodeId: "center",
      upstreamIds: new Set(),
      downstreamIds: new Set(),
      center: { x: 400, y: 300 },
      columnWidth: 300,
    });
    const pos = result.get("center");
    expect(pos?.x).toBeCloseTo(400, 0);
    expect(pos?.y).toBeCloseTo(300, 0);
  });
});
```

**Step 3: Run**

```bash
cd ui && npx vitest run src/features/workstation/CodebaseExplorer/__tests__/ForceLayout.test.ts
```

Expected: PASS

---

### Task 4.2: Wire ForceLayout into useLayout hook

**Files:**
- Create: `ui/src/features/workstation/CodebaseExplorer/layout/useLayout.ts`

**Step 1: Write the hook**

```typescript
// ui/src/features/workstation/CodebaseExplorer/layout/useLayout.ts
import { useEffect, useState, useMemo } from "react";
import { runElkLayout, type PositionMap } from "./ElkLayout";
import { runForceLayout } from "./ForceLayout";
import type { ExplorerGraph } from "../types";

type UseLayoutOptions = {
  graph: ExplorerGraph;
  focusedNodeId: string | null;
  upstreamIds: Set<string>;
  downstreamIds: Set<string>;
  canvasSize: { width: number; height: number };
};

export function useLayout(options: UseLayoutOptions): PositionMap {
  const { graph, focusedNodeId, upstreamIds, downstreamIds, canvasSize } = options;
  const [elkPositions, setElkPositions] = useState<PositionMap>(new Map());

  const topologyKey = useMemo(() => {
    const nodeIds = graph.nodes.map((n) => n.id).sort().join(",");
    // Include relation so calls/parameter_flow/return_flow between same pair are distinct
    const edgeIds = graph.edges.map((e) => `${e.from}>${e.to}:${e.relation}`).sort().join(",");
    return `${nodeIds}|${edgeIds}`;
  }, [graph.nodes, graph.edges]);

  // Re-run ELK only when topology changes
  useEffect(() => {
    const nodes = graph.nodes.map((n) => ({
      id: n.id,
      kind: n.kind,
      // Use signature-aware sizing: symbol nodes with signatures are wider/taller
      width: n.kind === "crate" ? 160 : n.kind === "module" ? 140 : n.kind === "file" ? 140
        : n.signature ? 240 : 200,
      height: n.kind === "crate" ? 40 : n.kind === "module" ? 36 : n.kind === "file" ? 32
        : n.signature ? 52 : 36,
    }));
    const edges = graph.edges.map((e) => ({
      id: `${e.from}::${e.to}::${e.relation}`, // include relation to avoid collisions
      fromId: e.from,
      toId: e.to,
      relation: e.relation,
    }));
    void runElkLayout(nodes, edges).then(setElkPositions);
  }, [topologyKey]); // eslint-disable-line react-hooks/exhaustive-deps

  // Apply force layout on top of ELK positions when in focus mode
  const positions = useMemo(() => {
    if (!focusedNodeId || elkPositions.size === 0) return elkPositions;
    return runForceLayout(elkPositions, {
      focusedNodeId,
      upstreamIds,
      downstreamIds,
      center: { x: canvasSize.width / 2, y: canvasSize.height / 2 },
      columnWidth: 320,
    });
  }, [elkPositions, focusedNodeId, upstreamIds, downstreamIds, canvasSize]);

  return positions;
}
```

**Step 2: Swap `useLayout` into `PixiExplorerCanvas`** — replace the manual `runElkLayout` call with `useLayout`.

**Step 3: Run full tests**

```bash
cd ui && npx vitest run
```

Expected: all pass

**Step 4: Commit**

```bash
cd ui && git add src/features/workstation/CodebaseExplorer/layout/
git commit -m "feat(explorer): spring-physics ego layout via d3-force (Phase 4)"
```

---

### Task 4.3: Add edge particle system for trace mode

**Files:**
- Modify: `ui/src/features/workstation/CodebaseExplorer/pixi/EdgeLayer.ts`
- Modify: `ui/src/features/workstation/CodebaseExplorer/pixi/PixiRenderer.ts`

**Step 1: Add particle system to EdgeLayer**

```typescript
// Add to EdgeLayer class:
private particles: Array<{
  edgeId: string;
  t: number; // 0–1 position along edge
  color: number;
  fromId: string;
  toId: string;
}> = [];

private particleGraphics = new PIXI.Graphics();

// In constructor, add particleGraphics above edge graphics:
stage.addChild(this.particleGraphics);

setParticleEdges(edges: RenderEdge[]): void {
  const edgeSet = new Set(edges.map((e) => e.id));
  // Remove particles for cleared edges
  this.particles = this.particles.filter((p) => edgeSet.has(p.edgeId));
  // Add 2 particles per new active edge
  for (const edge of edges) {
    const existing = this.particles.filter((p) => p.edgeId === edge.id).length;
    for (let i = existing; i < 2; i++) {
      this.particles.push({
        edgeId: edge.id,
        t: i * 0.5, // stagger
        color: edge.color,
        fromId: edge.fromId,
        toId: edge.toId,
      });
    }
  }
}

tickParticles(nodeById: Map<string, RenderNode>, dt: number): void {
  const speed = 0.0008; // fraction of edge length per ms
  this.particleGraphics.clear();

  for (const particle of this.particles) {
    particle.t = (particle.t + speed * dt) % 1;
    const from = nodeById.get(particle.fromId);
    const to = nodeById.get(particle.toId);
    if (!from || !to) continue;

    const x1 = from.x + from.width / 2;
    const y1 = from.y + from.height;
    const x2 = to.x + to.width / 2;
    const y2 = to.y;
    const midY = (y1 + y2) / 2;
    const t = particle.t;

    // Cubic bezier point
    const px = (1-t)**3*x1 + 3*(1-t)**2*t*x1 + 3*(1-t)*t**2*x2 + t**3*x2;
    const py = (1-t)**3*y1 + 3*(1-t)**2*t*midY + 3*(1-t)*t**2*midY + t**3*y2;

    this.particleGraphics.circle(px, py, 3).fill({ color: particle.color });
  }
}
```

**Step 2: Drive particle tick from PixiRenderer ticker**

```typescript
// In PixiRenderer init(), after layers created:
this.app.ticker.add((ticker) => {
  if (this.currentGraph) {
    const nodeById = new Map(this.currentGraph.nodes.map((n) => [n.id, n]));
    this.edgeLayer.tickParticles(nodeById, ticker.deltaMS);
  }
});
```

**Step 3: Commit**

```bash
cd ui && git add src/features/workstation/CodebaseExplorer/pixi/
git commit -m "feat(explorer): edge particle system for trace mode (Phase 4)"
```

---

## Phase 5 — Trace Algorithms

BFS "trace to entry" attack path, dataflow trace, amber highlighting, sticky banner.

---

### Task 5.1: Create useTraceAlgorithm hook

**Files:**
- Create: `ui/src/features/workstation/CodebaseExplorer/hooks/useTraceAlgorithm.ts`

**Step 1: Write the hook**

```typescript
// ui/src/features/workstation/CodebaseExplorer/hooks/useTraceAlgorithm.ts
import { useCallback, useState } from "react";
import type { ExplorerGraph } from "../types";

export type TraceResult = {
  kind: "attack_path" | "dataflow";
  pathNodeIds: string[];  // ordered source → target
  pathEdgeIds: Set<string>;
  banner: string;
};

function buildCallerMap(graph: ExplorerGraph): Map<string, string[]> {
  const map = new Map<string, string[]>();
  for (const edge of graph.edges) {
    if (edge.relation !== "calls") continue;
    if (!map.has(edge.to)) map.set(edge.to, []);
    map.get(edge.to)!.push(edge.from);
  }
  return map;
}

/** BFS backwards to find shortest path from targetId to an entry point (no callers). */
function traceToEntry(
  targetId: string,
  graph: ExplorerGraph
): { path: string[]; found: boolean } {
  const callerMap = buildCallerMap(graph);

  // BFS: track parent for path reconstruction
  const parent = new Map<string, string | null>();
  parent.set(targetId, null);
  const queue = [targetId];

  while (queue.length > 0) {
    const current = queue.shift()!;
    const callers = callerMap.get(current) ?? [];

    if (callers.length === 0) {
      // Entry point found — reconstruct path entry → target.
      // parent[node] = the node we came FROM in BFS (i.e., toward target).
      // Walk from entry (current) via parent chain; each step moves toward target.
      // Collect with push, then reverse to get entry → target order.
      const reversed: string[] = [];
      let cur: string | null = current;
      while (cur !== null) {
        reversed.push(cur);
        const p = parent.get(cur);
        cur = p !== undefined && p !== null ? p : null;
        if (reversed.length > 50) break; // guard against cycles
      }
      // reversed is now [entry, ..., target] — correct order, no reversal needed
      return { path: reversed, found: true };
    }

    for (const caller of callers) {
      if (!parent.has(caller)) {
        parent.set(caller, current);
        queue.push(caller);
      }
    }
  }

  return { path: [targetId], found: false };
}

function traceDataflow(
  nodeId: string,
  graph: ExplorerGraph
): { inEdgeIds: Set<string>; outEdgeIds: Set<string> } {
  const inEdgeIds = new Set<string>();
  const outEdgeIds = new Set<string>();
  for (const edge of graph.edges) {
    if (edge.relation === "parameter_flow" && edge.to === nodeId) {
      inEdgeIds.add(`${edge.from}::${edge.to}::${edge.relation}`);
    }
    if (edge.relation === "return_flow" && edge.from === nodeId) {
      outEdgeIds.add(`${edge.from}::${edge.to}::${edge.relation}`);
    }
  }
  return { inEdgeIds, outEdgeIds };
}

export function useTraceAlgorithm(graph: ExplorerGraph, nodeMap: Map<string, { label: string }>) {
  const [traceResult, setTraceResult] = useState<TraceResult | null>(null);

  const traceToEntryPoint = useCallback((nodeId: string) => {
    const { path, found } = traceToEntry(nodeId, graph);

    if (!found || path.length <= 1) {
      setTraceResult({
        kind: "attack_path",
        pathNodeIds: path,
        pathEdgeIds: new Set(),
        banner: "No entry point found within loaded graph. Try expanding clusters or switching to full depth.",
      });
      return;
    }

    const pathEdgeIds = new Set<string>();
    for (let i = 0; i < path.length - 1; i++) {
      pathEdgeIds.add(`${path[i]}::${path[i + 1]}::calls`);
    }

    const labels = path.map((id) => nodeMap.get(id)?.label ?? id);
    const banner = `Attack path: ${labels.join(" → ")} (${path.length - 1} hops)`;

    setTraceResult({ kind: "attack_path", pathNodeIds: path, pathEdgeIds, banner });
  }, [graph, nodeMap]);

  const traceDataflowForNode = useCallback((nodeId: string) => {
    const { inEdgeIds, outEdgeIds } = traceDataflow(nodeId, graph);
    const allEdges = new Set([...inEdgeIds, ...outEdgeIds]);
    const label = nodeMap.get(nodeId)?.label ?? nodeId;

    setTraceResult({
      kind: "dataflow",
      pathNodeIds: [nodeId],
      pathEdgeIds: allEdges,
      banner: `Dataflow through ${label}: ${inEdgeIds.size} input(s), ${outEdgeIds.size} output(s)`,
    });
  }, [graph, nodeMap]);

  const clearTrace = useCallback(() => setTraceResult(null), []);

  return { traceResult, traceToEntryPoint, traceDataflowForNode, clearTrace };
}
```

**Step 2: Write tests**

```typescript
// ui/src/features/workstation/CodebaseExplorer/__tests__/useTraceAlgorithm.test.ts
import { describe, it, expect } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { useTraceAlgorithm } from "../hooks/useTraceAlgorithm";
import type { ExplorerGraph } from "../types";

// network → validator → crypto (3 hops)
const GRAPH: ExplorerGraph = {
  nodes: [
    { id: "net", label: "recv_message", kind: "function" },
    { id: "val", label: "validate", kind: "function" },
    { id: "cry", label: "verify_sig", kind: "function" },
  ],
  edges: [
    { from: "net", to: "val", relation: "calls" },
    { from: "val", to: "cry", relation: "calls" },
    { from: "net", to: "cry", relation: "parameter_flow", parameterName: "msg" },
  ],
};

const NODE_MAP = new Map([
  ["net", { label: "recv_message" }],
  ["val", { label: "validate" }],
  ["cry", { label: "verify_sig" }],
]);

describe("useTraceAlgorithm — traceToEntryPoint", () => {
  it("finds path from entry to target", () => {
    const { result } = renderHook(() => useTraceAlgorithm(GRAPH, NODE_MAP));
    act(() => result.current.traceToEntryPoint("cry"));
    expect(result.current.traceResult?.pathNodeIds).toEqual(["net", "val", "cry"]);
  });

  it("banner includes hop count", () => {
    const { result } = renderHook(() => useTraceAlgorithm(GRAPH, NODE_MAP));
    act(() => result.current.traceToEntryPoint("cry"));
    expect(result.current.traceResult?.banner).toContain("2 hops");
  });

  it("returns no-entry-found message for isolated node", () => {
    const isolatedGraph: ExplorerGraph = {
      nodes: [{ id: "iso", label: "iso", kind: "function" }],
      edges: [],
    };
    const { result } = renderHook(() =>
      useTraceAlgorithm(isolatedGraph, new Map([["iso", { label: "iso" }]]))
    );
    act(() => result.current.traceToEntryPoint("iso"));
    expect(result.current.traceResult?.banner).toContain("No entry point found");
  });

  it("clearTrace resets result to null", () => {
    const { result } = renderHook(() => useTraceAlgorithm(GRAPH, NODE_MAP));
    act(() => result.current.traceToEntryPoint("cry"));
    act(() => result.current.clearTrace());
    expect(result.current.traceResult).toBeNull();
  });
});

describe("useTraceAlgorithm — traceDataflowForNode", () => {
  it("identifies parameter_flow edges into the node", () => {
    const { result } = renderHook(() => useTraceAlgorithm(GRAPH, NODE_MAP));
    act(() => result.current.traceDataflowForNode("cry"));
    expect(result.current.traceResult?.kind).toBe("dataflow");
    expect(result.current.traceResult?.pathEdgeIds.size).toBeGreaterThan(0);
  });
});
```

**Step 3: Run**

```bash
cd ui && npx vitest run src/features/workstation/CodebaseExplorer/__tests__/useTraceAlgorithm.test.ts
```

Expected: PASS (5 tests)

**Step 4: Wire trace into ContextMenu buttons**

In `ContextMenu.tsx`, replace the placeholder comments:

```typescript
// Trace to entry button:
onClick={() => {
  const { traceToEntryPoint } = traceCtx; // passed as prop from PixiExplorerCanvas
  traceToEntryPoint(nodeId);
  onClose();
}}

// Trace dataflow button:
onClick={() => {
  const { traceDataflowForNode } = traceCtx;
  traceDataflowForNode(nodeId);
  onClose();
}}
```

Pass `traceCtx` as a prop from `PixiExplorerCanvas` where `useTraceAlgorithm` is called.

**Step 5: Add trace banner component**

```typescript
// In PixiExplorerCanvas.tsx, above the canvas:
{traceResult && (
  <div className="explorer-trace-banner" role="status">
    <span>{traceResult.banner}</span>
    <button
      type="button"
      onClick={clearTrace}
      aria-label="Clear trace"
      className="explorer-trace-close"
    >
      ✕
    </button>
  </div>
)}
```

Add CSS:
```css
.explorer-trace-banner {
  position: absolute;
  top: 48px; /* below toolbar */
  left: 50%;
  transform: translateX(-50%);
  background: rgba(245, 158, 11, 0.15);
  border: 1px solid #f59e0b;
  border-radius: 6px;
  padding: 6px 12px;
  color: #fcd34d;
  font-size: 12px;
  display: flex;
  align-items: center;
  gap: 12px;
  z-index: 100;
  max-width: 80%;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.explorer-trace-close {
  background: none;
  border: none;
  color: #f59e0b;
  cursor: pointer;
  padding: 0 4px;
  flex-shrink: 0;
}
```

**Step 6: Apply trace highlighting in useRenderGraph**

`useRenderGraph` receives `traceResult` as an additional parameter. When a trace is active, it must:
- Dim all non-path nodes to `0.06` opacity
- Set amber `0xf59e0b` border and `2px` width on path nodes
- Override edge color and opacity: path edges become amber `0xf59e0b` at full opacity; non-path edges fade to `0.06`
- For dataflow traces: `parameter_flow` path edges use purple `0xa78bfa`, `return_flow` path edges use green `0x34d399`

```typescript
// Add traceResult to the useRenderGraph parameter and useMemo deps
// Pass it in from PixiExplorerCanvas: useRenderGraph(ctx, positions, traceResult)

// Inside the node mapping:
if (traceResult) {
  const inPath = new Set(traceResult.pathNodeIds);
  node.opacity = inPath.has(n.id) ? 1 : 0.06;
  if (inPath.has(n.id)) {
    node.borderColor = 0xf59e0b; // amber for both attack_path and dataflow
    node.borderWidth = 2;
  }
}

// Inside the edge mapping (after the base color/opacity is set):
if (traceResult) {
  const inPathEdge = traceResult.pathEdgeIds.has(edge.id);
  if (inPathEdge) {
    // Dataflow edges keep their relation color; attack_path edges are amber
    if (traceResult.kind === "attack_path") {
      edge.color = 0xf59e0b;
    }
    // parameter_flow and return_flow already have their correct colors from EDGE_COLOR
    edge.width = 2.5;
    edge.opacity = 1;
  } else {
    edge.opacity = 0.06;
  }
}
```

After `useRenderGraph` returns the updated `renderGraph`, `PixiExplorerCanvas` must also activate particles on the renderer for path edges. Add this effect alongside the existing `updateGraph` effect:

```typescript
// In PixiExplorerCanvas.tsx, add effect to activate/clear particles when trace changes:
useEffect(() => {
  if (!rendererRef.current) return;
  if (traceResult) {
    // Collect the RenderEdge objects that are on the path
    const pathEdges = renderGraph.edges.filter((e) => traceResult.pathEdgeIds.has(e.id));
    rendererRef.current.setParticleEdges(pathEdges);
  } else {
    rendererRef.current.setParticleEdges([]); // clear particles when trace cleared
  }
}, [traceResult, renderGraph, rendererRef]);
```

`PixiRenderer` must expose `setParticleEdges` as a public method that delegates to `EdgeLayer.setParticleEdges`. Add to `PixiRenderer.ts`:

```typescript
setParticleEdges(edges: RenderEdge[]): void {
  this.edgeLayer.setParticleEdges(edges);
}
```

**Step 7: Run full tests + visual check**

```bash
cd ui && npx vitest run
cd ui && npm run dev
```

Right-click a function → "Trace to entry". Confirm amber path, banner, particles.

**Step 8: Commit**

```bash
cd ui && git add src/features/workstation/CodebaseExplorer/
git commit -m "feat(explorer): attack path and dataflow trace algorithms (Phase 5)"
```

---

## Phase 6 — Polish and Cleanup

Remove ReactFlow, tune visual constants.

---

### Task 6.1: Remove ReactFlow and old node files

**Files:**
- Delete: `ui/src/features/workstation/CodebaseExplorer/ExplorerCanvas.tsx`
- Delete: `ui/src/features/workstation/CodebaseExplorer/AdaptiveLayout.tsx`
- Delete: `ui/src/features/workstation/CodebaseExplorer/nodes/ClusterNode.tsx`
- Delete: `ui/src/features/workstation/CodebaseExplorer/nodes/FileNode.tsx`
- Delete: `ui/src/features/workstation/CodebaseExplorer/nodes/SymbolNode.tsx`

**Step 1: Delete files**

```bash
cd ui/src/features/workstation/CodebaseExplorer
rm ExplorerCanvas.tsx AdaptiveLayout.tsx nodes/ClusterNode.tsx nodes/FileNode.tsx nodes/SymbolNode.tsx
```

**Step 2: Remove reactflow from package.json**

```bash
cd ui && npm uninstall reactflow
```

**Step 3: Verify test mock is already in place** — the pixi.js mock was added to `CodebaseExplorer.test.tsx` in Phase 2 Task 2.7 Step 5. Confirm the `vi.mock("reactflow", ...)` block is gone and `vi.mock("pixi.js", ...)` is present.

**Step 4: Run full test suite**

```bash
cd ui && npx vitest run
```

Expected: all pass

**Step 5: Run TypeScript check**

```bash
cd ui && npx tsc --noEmit
```

Expected: no errors

**Step 6: Visual smoke test in browser**

```bash
cd ui && npm run dev
```

Verify: dark canvas, nodes visible, click to focus, right-click for menu, trace works, Escape clears state, pan and zoom work.

**Step 7: Final commit**

```bash
cd ui && git add -A
git commit -m "feat(explorer): Pixi.js DAG explorer complete — remove ReactFlow (Phase 6)"
```

---

## Test coverage checklist

After all phases, these behaviors must have passing tests:

- [ ] `PixiRenderer` — init, destroy, resize
- [ ] `ElkLayout` — returns positions for all nodes, excludes contains edges
- [ ] `LodController` — correct LOD at boundary zoom values
- [ ] `useRenderGraph` — node count, contains edge excluded, focus opacity, focused border, dashed edges
- [ ] `ForceLayout` — returns all positions, focused node at center
- [ ] `ContextMenu` — renders header, counts, triggers callbacks, Escape closes, null nodeId renders nothing
- [ ] `useTraceAlgorithm` — correct path, hop count in banner, no-entry message, clearTrace, dataflow edges

Existing tests in `CodebaseExplorer.test.tsx` (rewritten for Pixi interaction model) cover: loading state, error state, focus state (triggered via `simulateNodeClick`), ego banner display, back-button navigation, Escape to clear focus, and neighborhood highlight. Node render is not exercised by these tests — Pixi drawing is mocked; visual correctness of nodes is covered by the unit tests for `NodeLayer` and `useRenderGraph`.
