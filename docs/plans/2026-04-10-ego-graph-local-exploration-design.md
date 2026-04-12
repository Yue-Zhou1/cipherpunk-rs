# Ego-Graph Local Exploration Design

**Date:** 2026-04-10  
**Status:** Approved

## Problem

The interactive DAG degrades significantly on large codebases — ELK layered layout blocks the main thread, all nodes render simultaneously with no virtualization, and cross-edges between modules produce overlapping lines that are unreadable. The root cause is that the global layout is not the right tool for local exploration.

## Goal

Make local (focused) graph exploration fast and visually clear at any codebase scale by replacing the global ELK layout in focus mode with a dedicated ego-graph column layout that only renders the neighborhood of the selected node.

## Approach: Ego-Graph Mode (Approach A)

When `stateKind === "focus"`, the canvas transitions to an ego-graph view showing only the focused node plus N hops of callers (left) and callees (right). The global graph disappears; overview mode is restored by clicking the background or the back button.

This builds entirely on the existing state machine (`stateKind: "overview" | "focus" | "trace"`) — no new states are needed.

## State Machine

| State | Layout | Nodes Shown |
|-------|--------|-------------|
| `overview` | ELK layered (unchanged) | All visible per granularity |
| `focus` | Custom column layout (new) | Ego-graph: focused node + N-hop neighborhood |
| `trace` | ELK layered (unchanged) | Trace neighborhood (unchanged) |

Transition into focus: user clicks any node (existing behavior).  
Transition out of focus: user clicks canvas background, or clicks "← Overview" in the banner (new).

## Column Layout Algorithm

```
[depth-2 callers] → [depth-1 callers] → [FOCUSED NODE] → [depth-1 callees] → [depth-2 callees]
```

Pure O(n) column assignment — no ELK, no external dependency:

1. BFS upstream from focused node using existing `upstreamIds` — assign column index `-depth`
2. BFS downstream from focused node using existing `downstreamIds` — assign column index `+depth`
3. Within each column, distribute nodes vertically: `y = rowIndex × ROW_HEIGHT`
4. `x = columnIndex × COLUMN_WIDTH`

**Constants:**
- `COLUMN_WIDTH = 380px`
- `ROW_HEIGHT = 100px`
- `MAX_NODES_PER_COLUMN = 8` — excess nodes replaced by a `+N more` summary chip
- `MAX_DEPTH_IN_EGO_MODE = 5` — depth slider capped at 5 in focus mode (vs 10 in overview)

## Node Visual Hierarchy

| Node | Size | Style |
|------|------|-------|
| Focused node | 320×96px | Green border (`--accent-green`), elevated z-index |
| Depth-1 neighbors | 280×80px | Full opacity |
| Depth-2+ neighbors | 280×80px | 70% opacity |
| `+N more` chip | 160×40px | Muted, non-interactive label |

## Focus Mode Banner

Appears between the toolbar and canvas when `stateKind === "focus"`:

```
[← Overview]  Exploring: function_name  (src/lib.rs:42)  Callers: 12  Callees: 7
```

- **← Overview**: calls `ctx.clearFocus()`, returns to overview
- **function_name**: from `focusedNode.label`
- **file path**: from `focusedNode.filePath + ":" + focusedNode.line`
- **Callers / Callees counts**: total available in graph (not capped by depth), so user knows when depth is truncating

## Interactions

| Action | Result |
|--------|--------|
| Click non-center node in ego mode | Re-centers ego-graph on that node (walks the call graph) |
| Click canvas background | Returns to overview |
| `+` / `-` depth buttons | Adds/removes outermost column pair; no full re-layout |
| Escape key | Returns to overview (existing behavior) |
| Right-click node | Context menu unchanged |
| MiniMap | Shows ego-graph only in focus mode |

## Architecture: Files Changed

### `ui/src/features/workstation/CodebaseExplorer/ExplorerCanvas.tsx`
- Add `egoLayout(nodes, focusedNodeId, upstreamIds, downstreamIds, depth)` function — pure column math, returns `Map<nodeId, {x, y}>`
- In the `useEffect` that triggers layout: branch on `ctx.stateKind === "focus"` → call `egoLayout` instead of `layoutWithElk`
- Ego layout is synchronous (O(n)), no async needed

### `ui/src/features/workstation/CodebaseExplorer/AdaptiveLayout.tsx`
- In `buildFlowModel()`: when in focus mode, filter visible nodes to ego-graph only (`focusedNodeId` + nodes in `upstreamIds` + `downstreamIds` within depth)
- Add `+N more` chip nodes at end of columns that exceed `MAX_NODES_PER_COLUMN`
- Mark focused node with CSS class `explorer-ego-center`

### `ui/src/features/workstation/CodebaseExplorer/index.tsx`
- Add `EgoBanner` component rendered between toolbar and canvas when `stateKind === "focus"`
- Reads: `ctx.focusedNodeId`, `ctx.nodeMap`, `ctx.upstreamIds`, `ctx.downstreamIds`, `ctx.clearFocus`

### `ui/src/features/workstation/CodebaseExplorer/hooks/useFocusContext.ts`
- Expose `totalUpstreamCount` and `totalDownstreamCount` — counts before depth cap — for the banner

### `ui/src/styles.css`
- `.explorer-ego-banner` — strip layout, back button, node info
- `.explorer-ego-center` — larger node, green border override on the ReactFlow node wrapper
- `.explorer-depth-chip` — the `+N more` summary node style

## Files NOT Changed

- `ExplorerProvider.tsx`, `ExplorerContext.tsx` — context/state unchanged
- `useUnifiedGraph.ts` — graph data loading unchanged
- `useTrace.ts` — trace mode unchanged
- `ContextPanel.tsx`, `NodeContextMenu.tsx` — panel behavior unchanged
- `WorkstationShell.tsx` — no shell changes needed
- Overview mode ELK path — untouched

## Performance Impact

| Metric | Before | After (focus mode) |
|--------|--------|-------------------|
| Node count rendered | Full graph (unbounded) | ≤ 80 (capped) |
| Layout algorithm | ELK async (main thread, O(n log n)) | Column math sync (O(n)) |
| Layout time (large graph) | 2-8 seconds | < 5ms |
| Re-layout on depth change | Full ELK re-run | Incremental column append |

Overview mode performance is unchanged. The fix only applies when a node is focused.

## Testing

- Unit test `egoLayout()` — verify column assignments for a known graph
- Unit test `buildFlowModel()` in focus mode — verify node filtering and `+N more` chip generation
- Integration test: click node → ego mode renders; click background → overview restored
- Integration test: depth `+1` adds a column; depth `-1` removes it
- Existing tests must all pass unchanged
