import { useCallback, useState } from "react";

import type { ExplorerGraph, TraceResult } from "../types";

function bfsTracePath(
  startId: string,
  graph: ExplorerGraph,
  parameterName: string | null,
  direction: "upstream" | "downstream"
): string[] | null {
  const parentMap = new Map<string, string>();
  const visited = new Set<string>([startId]);
  let frontier = [startId];
  let deepestNode = startId;

  while (frontier.length > 0) {
    const next: string[] = [];
    for (const current of frontier) {
      for (const edge of graph.edges) {
        const isRelevant =
          direction === "upstream"
            ? edge.to === current &&
              (edge.relation === "parameter_flow" || edge.relation === "calls")
            : edge.from === current &&
              (edge.relation === "return_flow" || edge.relation === "calls");

        if (!isRelevant) {
          continue;
        }

        if (
          direction === "upstream" &&
          parameterName &&
          edge.relation === "parameter_flow" &&
          edge.parameterName !== parameterName
        ) {
          continue;
        }

        const neighbor = direction === "upstream" ? edge.from : edge.to;
        if (visited.has(neighbor)) {
          continue;
        }

        visited.add(neighbor);
        parentMap.set(neighbor, current);
        next.push(neighbor);
        deepestNode = neighbor;
      }
    }
    frontier = next;
  }

  if (deepestNode === startId) {
    return null;
  }

  const path: string[] = [];
  let current: string | undefined = deepestNode;
  while (current !== undefined) {
    path.push(current);
    current = parentMap.get(current);
  }

  return direction === "upstream" ? path : path.reverse();
}

export function useTrace(graph: ExplorerGraph, focusedNodeId: string | null) {
  const [traceResult, setTraceResult] = useState<TraceResult | null>(null);

  const traceParameter = useCallback(
    (parameterName: string) => {
      if (!focusedNodeId) {
        return;
      }

      const path = bfsTracePath(focusedNodeId, graph, parameterName, "upstream");
      if (!path) {
        setTraceResult(null);
        return;
      }

      setTraceResult({
        path,
        direction: "upstream",
        parameterName,
      });
    },
    [focusedNodeId, graph]
  );

  const traceReturn = useCallback(() => {
    if (!focusedNodeId) {
      return;
    }

    const path = bfsTracePath(focusedNodeId, graph, null, "downstream");
    if (!path) {
      setTraceResult(null);
      return;
    }

    setTraceResult({
      path,
      direction: "downstream",
    });
  }, [focusedNodeId, graph]);

  const clearTrace = useCallback(() => {
    setTraceResult(null);
  }, []);

  return { traceResult, traceParameter, traceReturn, clearTrace };
}
