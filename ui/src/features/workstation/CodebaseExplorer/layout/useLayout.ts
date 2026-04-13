import { useEffect, useMemo, useState } from "react";

import type { ExplorerGraph } from "../types";
import { runElkLayout, type PositionMap } from "./ElkLayout";
import { runForceLayout } from "./ForceLayout";

type UseLayoutOptions = {
  graph: ExplorerGraph;
  focusedNodeId: string | null;
  upstreamIds: Set<string>;
  downstreamIds: Set<string>;
  canvasSize: { width: number; height: number };
};

function nodeDims(node: ExplorerGraph["nodes"][number]): { width: number; height: number } {
  if (node.kind === "crate") {
    return { width: 160, height: 40 };
  }
  if (node.kind === "module") {
    return { width: 140, height: 36 };
  }
  if (node.kind === "file") {
    return { width: 140, height: 32 };
  }
  if (node.signature) {
    return { width: 240, height: 52 };
  }
  return { width: 200, height: 36 };
}

export function useLayout(options: UseLayoutOptions): PositionMap {
  const { graph, focusedNodeId, upstreamIds, downstreamIds, canvasSize } = options;
  const [elkPositions, setElkPositions] = useState<PositionMap>(new Map());

  const topologyKey = useMemo(() => {
    const nodeIds = graph.nodes.map((node) => node.id).sort().join(",");
    const edgeIds = graph.edges
      .map((edge) => `${edge.from}>${edge.to}:${edge.relation}`)
      .sort()
      .join(",");
    return `${nodeIds}|${edgeIds}`;
  }, [graph.edges, graph.nodes]);

  useEffect(() => {
    const nodes = graph.nodes.map((node) => {
      const dims = nodeDims(node);
      return {
        id: node.id,
        kind: node.kind,
        width: dims.width,
        height: dims.height,
      };
    });
    const edges = graph.edges.map((edge) => ({
      id: `${edge.from}::${edge.to}::${edge.relation}`,
      fromId: edge.from,
      toId: edge.to,
      relation: edge.relation,
    }));

    let cancelled = false;
    void runElkLayout(nodes, edges).then((positions) => {
      if (!cancelled) {
        setElkPositions(positions);
      }
    });

    return () => {
      cancelled = true;
    };
  }, [topologyKey]); // topologyKey encodes all topology-relevant node/edge changes.

  return useMemo(() => {
    if (!focusedNodeId || elkPositions.size === 0) {
      return elkPositions;
    }

    return runForceLayout(elkPositions, {
      focusedNodeId,
      upstreamIds,
      downstreamIds,
      center: { x: canvasSize.width / 2, y: canvasSize.height / 2 },
      columnWidth: 320,
    });
  }, [canvasSize, downstreamIds, elkPositions, focusedNodeId, upstreamIds]);
}
