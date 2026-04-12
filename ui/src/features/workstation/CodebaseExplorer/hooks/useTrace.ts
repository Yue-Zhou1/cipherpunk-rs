import { useCallback, useState } from "react";

import type { ExplorerEdge, ExplorerGraph, NeighborhoodResult } from "../types";

function buildAdjacency(
  edges: ExplorerEdge[],
  direction: "upstream" | "downstream"
): Map<string, string[]> {
  const adjacency = new Map<string, string[]>();
  for (const edge of edges) {
    if (edge.relation !== "calls") {
      continue;
    }
    const key = direction === "upstream" ? edge.to : edge.from;
    const value = direction === "upstream" ? edge.from : edge.to;
    if (!adjacency.has(key)) {
      adjacency.set(key, []);
    }
    adjacency.get(key)?.push(value);
  }
  return adjacency;
}

function bfsNeighborhood(startId: string, adjacency: Map<string, string[]>): Set<string> {
  const visited = new Set<string>([startId]);
  let frontier = [startId];

  while (frontier.length > 0) {
    const next: string[] = [];
    for (const nodeId of frontier) {
      for (const neighbor of adjacency.get(nodeId) ?? []) {
        if (visited.has(neighbor)) {
          continue;
        }
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
    if (!focusedNodeId) {
      return;
    }
    const adjacency = buildAdjacency(graph.edges, "upstream");
    const reachable = bfsNeighborhood(focusedNodeId, adjacency);
    if (reachable.size <= 1) {
      setNeighborhoodResult(null);
      return;
    }
    setNeighborhoodResult({
      highlightedIds: reachable,
      direction: "upstream",
    });
  }, [focusedNodeId, graph.edges]);

  const showCallees = useCallback(() => {
    if (!focusedNodeId) {
      return;
    }
    const adjacency = buildAdjacency(graph.edges, "downstream");
    const reachable = bfsNeighborhood(focusedNodeId, adjacency);
    if (reachable.size <= 1) {
      setNeighborhoodResult(null);
      return;
    }
    setNeighborhoodResult({
      highlightedIds: reachable,
      direction: "downstream",
    });
  }, [focusedNodeId, graph.edges]);

  const clearHighlight = useCallback(() => {
    setNeighborhoodResult(null);
  }, []);

  return { neighborhoodResult, showCallers, showCallees, clearHighlight };
}
