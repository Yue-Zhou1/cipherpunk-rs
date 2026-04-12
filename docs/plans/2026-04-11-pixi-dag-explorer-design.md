# Interactive DAG Explorer — Pixi.js Redesign

**Date:** 2026-04-11  
**Status:** Approved  
**Scope:** Replace ReactFlow canvas with Pixi.js WebGL renderer; add attack path trace, dataflow trace, semantic zoom, spring-physics ego layout, and edge particle animations.

---

## 1. Context and Goal

The CodebaseExplorer DAG view is one step in the audit workflow. An analyst imports a codebase (e.g. `sigp/anchor` — ~148 Rust files, 25 crates, ~250–300 visible nodes at file granularity, ~800–1500 edges at full depth) and uses the graph to:

1. **Orient** — understand major components and their relationships
2. **Drill** — pick a suspicious boundary or entry point, go deeper
3. **Trace** — follow a specific value or call chain end-to-end

The current ReactFlow implementation fails at medium scale: nodes and edges lack contrast against the dark canvas, the ELK horizontal spread is illegible, and there is no attack path trace capability. The goal is to replace the renderer with Pixi.js (WebGL, 60fps at 300 nodes / 1500 edges) while preserving all existing data hooks and types.

---

## 2. Architecture

### Two-layer split

**Canvas layer — Pixi.js**  
A single `<canvas>` element owned by a `PixiRenderer` class. Receives a plain `RenderGraph` data structure and draws it. Emits plain events (`onNodeClick`, `onNodeRightClick`, `onPaneClick`). Knows nothing about React state, audit sessions, or graph semantics.

**State layer — React hooks (unchanged)**  
`useUnifiedGraph`, `useFocusContext`, `useTrace`, `useAdaptiveThresholds`, `useDepthControl`, and all existing types stay exactly as they are. A new thin adapter hook `useRenderGraph` maps `ExplorerContextValue → RenderGraph` and feeds it to the Pixi renderer.

**React UI layer — unchanged**  
All panels, toolbar, inspector sidebar remain normal React + existing CSS. Framer Motion handles panel animations. The context menu is a React component absolutely positioned inside `PixiExplorerCanvas` using `event.clientX/Y` from the canvas right-click event.

### Layout pipeline

```
ExplorerGraph (data)
  → ElkLayout.ts          — hierarchical DAG, deterministic, runs once per topology change
  → positions: {id → {x, y}}
  → ForceLayout.ts        — d3-force spring simulation (focus mode + cluster expand only)
  → position updates → PixiRenderer (draws each tick)
```

ELK owns initial layout and re-layout after granularity/expand changes. d3-force activates only during focus mode and cluster expansion — it runs for max 300 ticks then stops. No continuous simulation cost at idle.

### Dependency changes

| Remove | Add |
|---|---|
| `reactflow` | `pixi.js` |
| (keep) `elkjs` | `d3-force` |
| — | `framer-motion` (already present for panels) |

Matter.js is **not used**. d3-force is the correct tool for graph spring simulation; Matter.js is a rigid-body engine incompatible with force-directed layout.

---

## 3. Visual Design

### Canvas background

`#0a0f1a` — solid, slightly deeper than current `#0f172a` to maximise node contrast.

### Node color system

| Node type | Background | Border | Border width |
|---|---|---|---|
| Crate (cluster) | `#1e2d3d` | `#4a90d9` | 2px |
| Module (cluster) | `#172130` | `#3a6fa8` | 1.5px |
| File | `#0d2137` | `#3b82f6` | 1.5px |
| Symbol (fn / method) | `#141c2e` | `#475569` | 1px |

### Node sizing

| Node type | Width | Height |
|---|---|---|
| Crate | 160px | 40px |
| Module | 140px | 36px |
| File | 140px | 32px |
| Symbol (no signature) | 200px | 36px |
| Symbol (with signature) | 240px | 52px |

### Edge color system — relation-typed

| Relation | Color | Style | Width |
|---|---|---|---|
| `calls` | `#4a90d9` | solid | 1.5px |
| `parameter_flow` | `#a78bfa` | dashed | 1.5px |
| `return_flow` | `#34d399` | dashed | 1.5px |
| `cfg` | `#64748b` | solid | 1px |
| `contains` | hidden (layout only) | — | — |

### Typography

| Element | Color | Size | Weight |
|---|---|---|---|
| Node label | `#f1f5f9` | 13px | 500 |
| Signature text | `#94a3b8` | 11px | 400 |
| Crate/module label | `#cbd5e1` | 12px | 600 |

### Semantic zoom — levels of detail

Pan and zoom are handled by wheel events directly on the Pixi canvas. Each wheel event updates the camera (stage transform) and `currentZoom` in `PixiRenderer`, which immediately re-draws with the correct LOD. LOD switches snap (no fade) to avoid motion sickness during zoom.

| Zoom level | What renders |
|---|---|
| `< 0.4` (overview) | Cluster nodes only; file/symbol nodes hidden; edges are 1px hairlines; no labels |
| `0.4 – 0.8` (navigation) | File nodes show labels; symbol nodes show name only; edges show relation color |
| `> 0.8` (inspection) | Full signature text; edge labels appear on hover; `parameter_flow` value previews shown |

### Focus mode

- Focused node: `#ffffff` border, 2px; outer glow `rgba(255,255,255,0.15)`
- Callers (upstream): `#3b82f6` border tint, left column
- Callees (downstream): `#f97316` border tint, right column
- Non-ego nodes: opacity `0.08`
- Canvas background dims to `#060b14`

### Attack path trace (mode B — primary trace)

- Path edges: `#f59e0b` amber, 2.5px solid
- Path nodes: 4px amber left-border accent strip
- Animated particles flow along path edges, source → target direction
- Non-path nodes: opacity `0.06`
- Sticky banner: `"Attack path: network::recv → sig_collector::verify → bls::verify (4 hops)"` with `✕` dismiss

### Dataflow trace

- `parameter_flow` edges in: `#a78bfa` purple highlight
- `return_flow` edges out: `#34d399` green highlight
- Non-dataflow nodes: opacity `0.06`

---

## 4. Interaction Model

### Canvas states

**Overview (default)** — ELK hierarchical layout. Bloom-in on load (200ms + 30ms/layer stagger). Pan/zoom primary navigation. Hover shows full-path tooltip after 80ms delay.

**Cluster click** — left-click on a crate or module node calls `expandCluster` + `toggleCluster` to expand/collapse its children. Does not enter focus mode.

**Focus** — triggered by left-click on a file or symbol node. Ego-network expands with spring animation. Depth control (1–5) extends the neighborhood. Click background or `Escape` returns to overview.

**Trace** — triggered by right-click context menu.

### Context menu

```
┌─────────────────────────────┐
│  verify_signature           │  (header, non-interactive)
│  signature_collector/lib.rs │  (path, muted)
├─────────────────────────────┤
│  ↑  Show callers      (3)   │
│  ↓  Show callees      (7)   │
│  ⟿  Trace to entry         │
│  ⤳  Trace dataflow         │
├─────────────────────────────┤
│  ⌥  Open in editor         │
└─────────────────────────────┘
```

Rendered as a React component positioned absolutely inside `PixiExplorerCanvas` (fixed coordinates from `event.clientX/Y`). Framer Motion: scale `0.95→1.0`, opacity `0→1`, 100ms ease-out, origin at cursor.

### Trace to entry — algorithm

BFS backwards from selected node along `calls` edges until reaching a node with zero callers (entry point: network handler, public API, deserialized input). If multiple entry points, pick shortest path. Highlights linear chain in amber. If no entry point reachable: `"No entry point found within loaded graph. Try expanding clusters or switching to full depth."`.

---

## 5. Animation Budget

| Animation | Trigger | Duration | Easing |
|---|---|---|---|
| Graph bloom-in | Load | 200ms + 30ms/layer | ease-out |
| Node focus transition | Left-click | 300ms | spring (k=120, d=20) |
| Ego-network settle | Focus | 400ms | spring (k=80, d=18) |
| Dim non-ego | Focus | 150ms | ease-out |
| Return to overview | Escape / bg click | 200ms | ease-in-out |
| Cluster expand | Click cluster | 350ms | spring (k=100, d=22) |
| Trace highlight | Context menu | 120ms | ease-out |
| Edge particles | Active trace only | continuous 60fps | linear |
| Context menu | Right-click | 100ms | ease-out |

**What never animates:** zoom/pan (instant), LOD switch (snap), search/filter redraw (instant).

**Edge particles:** 3px circles in edge color (amber for attack path, purple/green for dataflow). 2–3 particles per edge, 80px/s. Dissolve over 200ms when trace cleared. Max ~40 particles on screen for a 15-hop path.

**Performance target:** 60fps at 300 visible nodes + 800 edges on mid-range laptop GPU.

- `ParticleContainer` for symbol and file nodes (batched draw calls)
- `Graphics` for cluster nodes
- Single `Graphics` for all edges, cleared/redrawn only when positions change
- d3-force runs max 300 ticks per activation, then stops

---

## 6. File Structure

### Deleted

```
CodebaseExplorer/
  ExplorerCanvas.tsx          — ReactFlow renderer
  AdaptiveLayout.tsx          — buildFlowModel, ReactFlow model construction
  nodes/ClusterNode.tsx
  nodes/FileNode.tsx
  nodes/SymbolNode.tsx
```

### Created

```
CodebaseExplorer/
  pixi/
    PixiRenderer.ts           — PIXI.Application, ticker, camera transform
    NodeLayer.ts              — node drawing, hover/click hit testing
    EdgeLayer.ts              — edge drawing, particle system
    LodController.ts          — semantic zoom LOD logic
    usePixiRenderer.ts        — React hook owning PixiRenderer lifecycle
  layout/
    ElkLayout.ts              — ELK runner, returns id→{x,y}
    ForceLayout.ts            — d3-force simulation, feeds PixiRenderer
    useLayout.ts              — ELK→d3-force orchestration
  hooks/
    useRenderGraph.ts         — ExplorerContextValue → RenderGraph adapter
    useTraceAlgorithm.ts      — BFS trace to entry, dataflow path finder
  ContextMenu.tsx             — React component, absolutely positioned, Framer Motion animated
  PixiExplorerCanvas.tsx      — NEW: thin shell mounting Pixi canvas + ContextMenu
```

### Unchanged

```
CodebaseExplorer/
  ExplorerContext.tsx
  ContextPanel.tsx
  TraceOverlay.tsx
  NodeContextMenu.tsx         — replaced by ContextMenu.tsx above
  hooks/useUnifiedGraph.ts
  hooks/useFocusContext.ts
  hooks/useTrace.ts
  hooks/useAdaptiveThresholds.ts
  hooks/useDepthControl.ts
  types.ts
  index.tsx
```

---

## 7. Migration Plan — 6 Phases

Each phase is independently testable and ships as its own PR. Phase 2 alone fixes the immediate visual problems (contrast, sizing) regardless of later phases.

**Phase 1 — Pixi shell**  
`PixiRenderer`, `usePixiRenderer`, new `ExplorerCanvas`. Mounts blank WebGL canvas in place of ReactFlow. Verifies initialization, resize handling, unmount cleanup.

**Phase 2 — Static rendering**  
`NodeLayer`, `EdgeLayer`, `ElkLayout`, `LodController`. Reads `ExplorerGraph` from context, runs ELK, draws nodes and edges with new color system and sizing. Semantic zoom LOD. No interaction yet. Verifies contrast, layout, zoom behavior.

**Phase 3 — Interaction wiring**  
`ContextMenu`, click/hover/right-click handlers. Left-click focus, right-click menu (callers, callees, open in editor). Uses existing `useFocusContext` — no new logic. Ego-network dimming and EgoBanner continue to work.

**Phase 4 — Animation**  
`ForceLayout`, `useLayout`, spring transitions, bloom-in, edge particles. All entries from the animation budget. Verify 60fps on Anchor-scale graph.

**Phase 5 — Trace algorithms**  
`useTraceAlgorithm`, "trace to entry", "trace dataflow", amber/purple/green edge highlighting, node dimming, sticky banner. Verified against known graph fixtures.

**Phase 6 — Polish and cleanup**  
Remove ReactFlow and unused node components. Tune spring constants, particle speed, LOD thresholds against real Anchor data. Final performance pass.

---

## 8. Out of Scope

- Node annotations (deferred to a later phase)
- Matter.js (not used — d3-force covers all layout physics needs)
- Persistent ambient particle effects (particles are trace-mode only)
- Graph export / screenshot
