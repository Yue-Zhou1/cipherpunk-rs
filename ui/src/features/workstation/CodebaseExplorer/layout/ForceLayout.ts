import * as d3 from "d3-force";

import type { PositionMap } from "./ElkLayout";

type ForceNode = {
  id: string;
  x: number;
  y: number;
  fx?: number | null;
  fy?: number | null;
};

export type ForceLayoutConfig = {
  focusedNodeId: string | null;
  upstreamIds: Set<string>;
  downstreamIds: Set<string>;
  center: { x: number; y: number };
  columnWidth: number;
};

export function runForceLayout(
  initialPositions: PositionMap,
  config: ForceLayoutConfig
): PositionMap {
  const nodes: ForceNode[] = Array.from(initialPositions.entries()).map(([id, position]) => ({
    id,
    x: position.x,
    y: position.y,
  }));

  if (config.focusedNodeId) {
    const focusedNode = nodes.find((node) => node.id === config.focusedNodeId);
    if (focusedNode) {
      focusedNode.fx = config.center.x;
      focusedNode.fy = config.center.y;
    }
  }

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
    .force(
      "link",
      d3
        .forceLink(links)
        .id((datum) => (datum as ForceNode).id)
        .strength(0.3)
        .distance(120)
    )
    .force("charge", d3.forceManyBody().strength(-200))
    .force("y", d3.forceY(config.center.y).strength(0.2))
    .stop();

  for (let tick = 0; tick < 300; tick += 1) {
    simulation.tick();
  }

  const result: PositionMap = new Map();
  for (const node of nodes) {
    result.set(node.id, { x: node.x, y: node.y });
  }
  return result;
}
