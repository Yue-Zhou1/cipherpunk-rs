import { useCallback, useMemo, useState } from "react";

import type { ExplorerGraph, ExplorerStateKind } from "../types";

function bfsNeighbors(startId: string, adjacency: Map<string, string[]>, maxDepth: number): Set<string> {
  const visited = new Set<string>();
  let frontier = [startId];

  for (let depth = 0; depth < maxDepth && frontier.length > 0; depth += 1) {
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

  const { upstreamAdjacency, downstreamAdjacency } = useMemo(() => {
    const upstream = new Map<string, string[]>();
    const downstream = new Map<string, string[]>();

    for (const edge of graph.edges) {
      if (
        edge.relation !== "calls" &&
        edge.relation !== "parameter_flow" &&
        edge.relation !== "return_flow"
      ) {
        continue;
      }

      if (!downstream.has(edge.from)) {
        downstream.set(edge.from, []);
      }
      downstream.get(edge.from)!.push(edge.to);

      if (!upstream.has(edge.to)) {
        upstream.set(edge.to, []);
      }
      upstream.get(edge.to)!.push(edge.from);
    }

    return { upstreamAdjacency: upstream, downstreamAdjacency: downstream };
  }, [graph]);

  const upstreamIds = useMemo(() => {
    if (!focusedNodeId) {
      return new Set<string>();
    }
    return bfsNeighbors(focusedNodeId, upstreamAdjacency, depth);
  }, [depth, focusedNodeId, upstreamAdjacency]);

  const downstreamIds = useMemo(() => {
    if (!focusedNodeId) {
      return new Set<string>();
    }
    return bfsNeighbors(focusedNodeId, downstreamAdjacency, depth);
  }, [depth, downstreamAdjacency, focusedNodeId]);

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
