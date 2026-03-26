import { MarkerType, Position, type Edge, type Node } from "reactflow";

import type { ExplorerGraph, ExplorerNode } from "./types";

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

function parentModuleId(node: ExplorerNode): string | null {
  if (!node.filePath) {
    return null;
  }
  const lastSlash = node.filePath.lastIndexOf("/");
  if (lastSlash <= 0) {
    return null;
  }
  return `module:${node.filePath.slice(0, lastSlash)}`;
}

function parentCrateId(node: ExplorerNode): string | null {
  if (node.kind === "module") {
    const marker = "/src/";
    const markerIndex = node.id.indexOf(marker);
    if (markerIndex <= 0) {
      return null;
    }
    return node.id.slice(0, markerIndex);
  }
  if (!node.filePath) {
    return null;
  }

  const parts = node.filePath.split("/");
  if (parts.length < 2) {
    return null;
  }
  return `module:${parts[0]}/${parts[1]}`;
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

function countChildren(nodeId: string, graph: ExplorerGraph): number {
  return graph.edges.filter((edge) => edge.relation === "contains" && edge.from === nodeId).length;
}

function isVisibleNode(node: ExplorerNode, config: LayoutConfig): boolean {
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
      const parent = parentModuleId(node);
      return !parent || config.expandedClusters.has(parent);
    }
    case "crates": {
      if (node.kind === "crate") {
        return true;
      }
      if (node.kind === "module") {
        const crate = parentCrateId(node);
        return !!crate && config.expandedClusters.has(crate);
      }
      const crate = parentCrateId(node);
      const module = parentModuleId(node);
      if (module && config.expandedClusters.has(module)) {
        return true;
      }
      return !!crate && config.expandedClusters.has(crate) && !module;
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

  for (const node of graph.nodes) {
    if (!isVisibleNode(node, config)) {
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
        childCount: isCluster ? countChildren(node.id, graph) : undefined,
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
