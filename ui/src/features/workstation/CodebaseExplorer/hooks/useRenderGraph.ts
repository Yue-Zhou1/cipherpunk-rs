import { useMemo } from "react";

import type { PositionMap } from "../layout/ElkLayout";
import type { RenderEdge, RenderGraph, RenderNode } from "../pixi/types";
import type { ExplorerContextValue, ExplorerEdgeRelation, ExplorerNode } from "../types";

const SYMBOL_BG = 0x141c2e;
const SYMBOL_BORDER = 0x475569;

function nodeDims(node: ExplorerNode): { width: number; height: number } {
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

function nodeBgColor(kind: ExplorerNode["kind"]): number {
  if (kind === "crate") {
    return 0x1e2d3d;
  }
  if (kind === "module") {
    return 0x172130;
  }
  if (kind === "file") {
    return 0x0d2137;
  }
  return SYMBOL_BG;
}

function nodeBorderColor(kind: ExplorerNode["kind"]): number {
  if (kind === "crate") {
    return 0x4a90d9;
  }
  if (kind === "module") {
    return 0x3a6fa8;
  }
  if (kind === "file") {
    return 0x3b82f6;
  }
  return SYMBOL_BORDER;
}

function relationColor(relation: ExplorerEdgeRelation): number {
  if (relation === "calls") {
    return 0x4a90d9;
  }
  if (relation === "parameter_flow") {
    return 0xa78bfa;
  }
  if (relation === "return_flow") {
    return 0x34d399;
  }
  if (relation === "cfg") {
    return 0x64748b;
  }
  return 0x000000;
}

type RenderGraphInput = Pick<
  ExplorerContextValue,
  | "graph"
  | "focusedNodeId"
  | "upstreamIds"
  | "downstreamIds"
  | "stateKind"
  | "neighborhoodResult"
  | "matchingNodeIds"
>;

export function useRenderGraph(ctx: RenderGraphInput, positions: PositionMap): RenderGraph {
  return useMemo(() => {
    const nodes: RenderNode[] = ctx.graph.nodes.map((node) => {
      const pos = positions.get(node.id) ?? { x: 0, y: 0 };
      const dims = nodeDims(node);
      const isFocused = ctx.focusedNodeId === node.id;
      const isEgoUpstream = ctx.upstreamIds.has(node.id);
      const isEgoDownstream = ctx.downstreamIds.has(node.id);

      let opacity = 1;
      if (ctx.stateKind === "focus") {
        const inEgo = isFocused || isEgoUpstream || isEgoDownstream;
        opacity = inEgo ? 1 : 0.08;
      } else if (ctx.stateKind === "highlight" && ctx.neighborhoodResult) {
        opacity = ctx.neighborhoodResult.highlightedIds.has(node.id) ? 1 : 0.08;
      }

      if (ctx.matchingNodeIds && !ctx.matchingNodeIds.has(node.id)) {
        opacity = Math.min(opacity, 0.1);
      }

      let borderColor = nodeBorderColor(node.kind);
      let borderWidth = node.kind === "crate" ? 2 : node.kind === "module" ? 1.5 : 1;
      if (isFocused) {
        borderColor = 0xffffff;
        borderWidth = 2;
      } else if (isEgoUpstream) {
        borderColor = 0x3b82f6;
      } else if (isEgoDownstream) {
        borderColor = 0xf97316;
      }

      return {
        id: node.id,
        label: node.label,
        kind: node.kind,
        x: pos.x,
        y: pos.y,
        width: dims.width,
        height: dims.height,
        opacity,
        borderColor,
        borderWidth,
        bgColor: nodeBgColor(node.kind),
        isFocused,
        isEgoUpstream,
        isEgoDownstream,
        signature: node.signature
          ? {
              params: node.signature.parameters,
              returnType: node.signature.returnType,
            }
          : undefined,
      };
    });

    const edges: RenderEdge[] = ctx.graph.edges
      .filter((edge) => edge.relation !== "contains")
      .map((edge) => ({
        id: `${edge.from}::${edge.to}::${edge.relation}`,
        fromId: edge.from,
        toId: edge.to,
        relation: edge.relation,
        color: relationColor(edge.relation),
        width: edge.relation === "cfg" ? 1 : 1.5,
        dashed: edge.relation === "parameter_flow" || edge.relation === "return_flow",
        opacity: 1,
        // TODO(Phase 5): set true for traced edges to activate particle animation.
        hasParticle: false,
      }));

    return { nodes, edges };
  }, [ctx, positions]);
}
