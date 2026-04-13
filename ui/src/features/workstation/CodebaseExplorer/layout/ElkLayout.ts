import ELK from "elkjs/lib/elk.bundled.js";

import type { RenderEdge, RenderNode } from "../pixi/types";

const elk = new ELK();

export type PositionMap = Map<string, { x: number; y: number }>;

export async function runElkLayout(
  nodes: Pick<RenderNode, "id" | "kind" | "width" | "height">[],
  edges: Pick<RenderEdge, "id" | "fromId" | "toId" | "relation">[]
): Promise<PositionMap> {
  const visibleEdges = edges.filter((edge) => edge.relation !== "contains");

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
    children: nodes.map((node) => ({
      id: node.id,
      width: node.width,
      height: node.height,
    })),
    edges: visibleEdges.map((edge) => ({
      id: edge.id,
      sources: [edge.fromId],
      targets: [edge.toId],
    })),
  });

  const positions: PositionMap = new Map();
  for (const child of layout.children ?? []) {
    positions.set(child.id, { x: child.x ?? 0, y: child.y ?? 0 });
  }

  return positions;
}
