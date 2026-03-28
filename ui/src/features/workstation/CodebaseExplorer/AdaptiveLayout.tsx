import { MarkerType, Position, type Edge, type Node } from "reactflow";

import type { ExplorerEdge, ExplorerGraph, ExplorerNode } from "./types";

type LayoutConfig = {
  resolvedGranularity: "files" | "modules" | "crates";
  expandedClusters: Set<string>;
  focusedNodeId: string | null;
  upstreamIds: Set<string>;
  downstreamIds: Set<string>;
  tracePathIds: Set<string> | null;
  matchingNodeIds: Set<string> | null;
  stateKind: "overview" | "focus" | "trace";
};

export type FlowModel = {
  nodes: Node[];
  edges: Edge[];
};

/**
 * Build a parent lookup from "contains" edges.
 * key = child node ID, value = parent node ID.
 */
function buildParentMap(edges: ExplorerEdge[]): Map<string, string> {
  const map = new Map<string, string>();
  for (const edge of edges) {
    if (edge.relation === "contains") {
      map.set(edge.to, edge.from);
    }
  }
  return map;
}

function findAncestorByKind(
  nodeId: string,
  kind: ExplorerNode["kind"],
  parentMap: Map<string, string>,
  nodeById: Map<string, ExplorerNode>
): string | null {
  let current = parentMap.get(nodeId);
  while (current) {
    const node = nodeById.get(current);
    if (node?.kind === kind) {
      return current;
    }
    current = parentMap.get(current);
  }
  return null;
}

function nodeHighlightClass(nodeId: string, config: LayoutConfig): string {
  const classes: string[] = [];

  if (config.stateKind !== "overview") {
    if (nodeId === config.focusedNodeId) {
      classes.push("explorer-focused");
    } else if (config.tracePathIds?.has(nodeId)) {
      classes.push("explorer-trace");
    } else if (config.upstreamIds.has(nodeId)) {
      classes.push("explorer-upstream");
    } else if (config.downstreamIds.has(nodeId)) {
      classes.push("explorer-downstream");
    } else {
      classes.push("explorer-dimmed");
    }
  }

  if (config.matchingNodeIds && !config.matchingNodeIds.has(nodeId)) {
    classes.push("explorer-search-dimmed");
  }

  return classes.join(" ");
}

function countChildren(node: ExplorerNode, edges: ExplorerEdge[]): number {
  if (node.childCount != null) {
    return node.childCount;
  }
  return edges.filter((edge) => edge.relation === "contains" && edge.from === node.id).length;
}

function isVisibleNode(
  node: ExplorerNode,
  config: LayoutConfig,
  parentMap: Map<string, string>,
  nodeById: Map<string, ExplorerNode>
): boolean {
  switch (config.resolvedGranularity) {
    case "files": {
      return node.kind !== "crate" && node.kind !== "module";
    }
    case "modules": {
      if (node.kind === "crate") {
        return false;
      }
      if (node.kind === "module") {
        return true;
      }
      const parentModule = findAncestorByKind(node.id, "module", parentMap, nodeById);
      return !parentModule || config.expandedClusters.has(parentModule);
    }
    case "crates": {
      if (node.kind === "crate") {
        return true;
      }
      if (node.kind === "module") {
        const parentCrate = findAncestorByKind(node.id, "crate", parentMap, nodeById);
        return !!parentCrate && config.expandedClusters.has(parentCrate);
      }
      const parentModule = findAncestorByKind(node.id, "module", parentMap, nodeById);
      const parentCrate = findAncestorByKind(node.id, "crate", parentMap, nodeById);
      if (parentModule && config.expandedClusters.has(parentModule)) {
        return true;
      }
      return !!parentCrate && config.expandedClusters.has(parentCrate) && !parentModule;
    }
    default: {
      return true;
    }
  }
}

export function buildFlowModel(graph: ExplorerGraph, config: LayoutConfig): FlowModel {
  const nodes: Node[] = [];
  const edges: Edge[] = [];
  const visibleNodeIds = new Set<string>();
  const nodeById = new Map(graph.nodes.map((node) => [node.id, node]));
  const parentMap = buildParentMap(graph.edges);

  for (const node of graph.nodes) {
    if (!isVisibleNode(node, config, parentMap, nodeById)) {
      continue;
    }

    visibleNodeIds.add(node.id);
    const isCluster = node.kind === "crate" || node.kind === "module";
    const classes = nodeHighlightClass(node.id, config);

    nodes.push({
      id: node.id,
      type: isCluster ? "clusterNode" : node.kind === "file" ? "fileNode" : "symbolNode",
      position: { x: 0, y: 0 },
      sourcePosition: Position.Bottom,
      targetPosition: Position.Top,
      className: classes || undefined,
      data: {
        label: node.label,
        kind: node.kind,
        filePath: node.filePath,
        line: node.line,
        signature: node.signature,
        childCount: isCluster ? countChildren(node, graph.edges) : undefined,
        expanded: config.expandedClusters.has(node.id),
      },
    });
  }

  const edgeKeys = new Set<string>();
  for (const edge of graph.edges) {
    if (!visibleNodeIds.has(edge.from) || !visibleNodeIds.has(edge.to)) {
      continue;
    }
    if (edge.from === edge.to) {
      continue;
    }

    const key = `${edge.from}::${edge.to}::${edge.relation}`;
    if (edgeKeys.has(key)) {
      continue;
    }
    edgeKeys.add(key);

    const isTraceEdge = !!(
      config.tracePathIds?.has(edge.from) &&
      config.tracePathIds?.has(edge.to)
    );
    const isFlowEdge = edge.relation === "parameter_flow" || edge.relation === "return_flow";
    const dimmed =
      config.stateKind !== "overview" &&
      !isTraceEdge &&
      edge.from !== config.focusedNodeId &&
      edge.to !== config.focusedNodeId &&
      !config.upstreamIds.has(edge.from) &&
      !config.downstreamIds.has(edge.from) &&
      !config.upstreamIds.has(edge.to) &&
      !config.downstreamIds.has(edge.to);

    edges.push({
      id: key,
      source: edge.from,
      target: edge.to,
      type: "smoothstep",
      animated: isTraceEdge || isFlowEdge,
      label: edge.relation === "contains" ? undefined : edge.relation,
      markerEnd: {
        type: MarkerType.ArrowClosed,
        width: 16,
        height: 16,
        color: isTraceEdge ? "#8b5cf6" : "#5c7394",
      },
      style: {
        stroke: isTraceEdge ? "#8b5cf6" : "#5c7394",
        strokeWidth: isTraceEdge ? 2.5 : 1.4,
        opacity: dimmed ? 0.15 : 1,
      },
    });
  }

  return { nodes, edges };
}
