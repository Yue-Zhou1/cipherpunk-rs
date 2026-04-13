import { useCallback, useState } from "react";

import type { ExplorerGraph } from "../types";

export type TraceResult = {
  kind: "attack_path" | "dataflow";
  pathNodeIds: string[];
  pathEdgeIds: Set<string>;
  banner: string;
};

function edgeId(from: string, to: string, relation: string): string {
  return `${from}::${to}::${relation}`;
}

function buildCallerMap(graph: ExplorerGraph): Map<string, string[]> {
  const map = new Map<string, string[]>();
  for (const edge of graph.edges) {
    if (edge.relation !== "calls") {
      continue;
    }
    const callers = map.get(edge.to);
    if (callers) {
      callers.push(edge.from);
    } else {
      map.set(edge.to, [edge.from]);
    }
  }
  return map;
}

function findPathToEntry(targetId: string, graph: ExplorerGraph): { path: string[]; found: boolean } {
  const callerMap = buildCallerMap(graph);
  const parent = new Map<string, string | null>([[targetId, null]]);
  const queue = [targetId];

  while (queue.length > 0) {
    const current = queue.shift() as string;
    const callers = callerMap.get(current) ?? [];

    if (callers.length === 0) {
      const path: string[] = [];
      let cursor: string | null = current;
      while (cursor !== null) {
        path.push(cursor);
        cursor = parent.get(cursor) ?? null;
      }
      return { path, found: true };
    }

    for (const caller of callers) {
      if (parent.has(caller)) {
        continue;
      }
      parent.set(caller, current);
      queue.push(caller);
    }
  }

  return { path: [targetId], found: false };
}

function traceDataflow(nodeId: string, graph: ExplorerGraph): Set<string> {
  const selected = new Set<string>();
  for (const edge of graph.edges) {
    if (edge.relation === "parameter_flow" && edge.to === nodeId) {
      selected.add(edgeId(edge.from, edge.to, edge.relation));
    }
    if (edge.relation === "return_flow" && edge.from === nodeId) {
      selected.add(edgeId(edge.from, edge.to, edge.relation));
    }
  }
  return selected;
}

export function useTraceAlgorithm(
  graph: ExplorerGraph,
  nodeMap: Map<string, { label: string }>
) {
  const [traceResult, setTraceResult] = useState<TraceResult | null>(null);

  const traceToEntryPoint = useCallback(
    (nodeId: string) => {
      const { path, found } = findPathToEntry(nodeId, graph);

      if (!found || path.length <= 1) {
        setTraceResult({
          kind: "attack_path",
          pathNodeIds: path,
          pathEdgeIds: new Set<string>(),
          banner:
            "No entry point found within loaded graph. Try expanding clusters or switching to full depth.",
        });
        return;
      }

      const pathEdgeIds = new Set<string>();
      for (let index = 0; index < path.length - 1; index += 1) {
        pathEdgeIds.add(edgeId(path[index], path[index + 1], "calls"));
      }

      const labels = path.map((id) => nodeMap.get(id)?.label ?? id);
      setTraceResult({
        kind: "attack_path",
        pathNodeIds: path,
        pathEdgeIds,
        banner: `Attack path: ${labels.join(" -> ")} (${path.length - 1} hops)`,
      });
    },
    [graph, nodeMap]
  );

  const traceDataflowForNode = useCallback(
    (nodeId: string) => {
      const pathEdgeIds = traceDataflow(nodeId, graph);
      const label = nodeMap.get(nodeId)?.label ?? nodeId;

      let inputCount = 0;
      let outputCount = 0;
      for (const edge of graph.edges) {
        if (edge.relation === "parameter_flow" && edge.to === nodeId) {
          inputCount += 1;
        } else if (edge.relation === "return_flow" && edge.from === nodeId) {
          outputCount += 1;
        }
      }

      setTraceResult({
        kind: "dataflow",
        pathNodeIds: [nodeId],
        pathEdgeIds,
        banner: `Dataflow through ${label}: ${inputCount} input(s), ${outputCount} output(s)`,
      });
    },
    [graph, nodeMap]
  );

  const clearTrace = useCallback(() => {
    setTraceResult(null);
  }, []);

  return { traceResult, traceToEntryPoint, traceDataflowForNode, clearTrace };
}
