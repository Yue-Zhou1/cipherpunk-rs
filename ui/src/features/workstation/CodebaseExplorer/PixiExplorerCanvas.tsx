import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { ContextMenu } from "./ContextMenu";
import { useExplorer } from "./ExplorerContext";
import { useRenderGraph } from "./hooks/useRenderGraph";
import { runElkLayout, type PositionMap } from "./layout/ElkLayout";
import { usePixiRenderer } from "./pixi/usePixiRenderer";

export function PixiExplorerCanvas() {
  const ctx = useExplorer();
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [positions, setPositions] = useState<PositionMap>(new Map());
  const [contextMenu, setContextMenu] = useState<{
    nodeId: string;
    x: number;
    y: number;
  } | null>(null);
  const renderGraph = useRenderGraph(ctx, positions);

  const handleNodeClick = useCallback(
    (nodeId: string) => {
      setContextMenu(null);
      const node = ctx.nodeMap.get(nodeId);
      if (node?.kind === "crate" || node?.kind === "module") {
        // Deliberately do both:
        // 1) request data load for the cluster (expandCluster)
        // 2) toggle local visibility state for already-loaded children (toggleCluster)
        ctx.expandCluster(nodeId);
        ctx.toggleCluster(nodeId);
      } else {
        ctx.focusNode(nodeId);
      }
    },
    [ctx]
  );

  const handleNodeRightClick = useCallback(
    (nodeId: string, x: number, y: number) => {
      ctx.focusNode(nodeId);
      setContextMenu({ nodeId, x, y });
    },
    [ctx]
  );

  const handlePaneClick = useCallback(() => {
    setContextMenu(null);
    if (ctx.neighborhoodResult) {
      ctx.clearHighlight();
    } else if (ctx.stateKind === "focus") {
      ctx.clearFocus();
    }
  }, [ctx]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") {
        return;
      }
      setContextMenu(null);
      if (ctx.neighborhoodResult) {
        ctx.clearHighlight();
      } else if (ctx.stateKind === "focus") {
        ctx.clearFocus();
      }
    };

    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [ctx]);

  const rendererRef = usePixiRenderer(canvasRef, {
    onNodeClick: handleNodeClick,
    onNodeRightClick: handleNodeRightClick,
    onPaneClick: handlePaneClick,
  });

  const topologyKey = useMemo(() => {
    const nodeIds = ctx.graph.nodes.map((node) => node.id).sort().join(",");
    const edgeIds = ctx.graph.edges
      .map((edge) => `${edge.from}>${edge.to}:${edge.relation}`)
      .sort()
      .join(",");
    return `${nodeIds}|${edgeIds}`;
  }, [ctx.graph.edges, ctx.graph.nodes]);

  useEffect(() => {
    const stubNodes = ctx.graph.nodes.map((node) => ({
      id: node.id,
      kind: node.kind,
      width:
        node.kind === "crate"
          ? 160
          : node.kind === "module"
            ? 140
            : node.kind === "file"
              ? 140
              : node.signature
                ? 240
                : 200,
      height:
        node.kind === "crate"
          ? 40
          : node.kind === "module"
            ? 36
            : node.kind === "file"
              ? 32
              : node.signature
                ? 52
                : 36,
    }));
    const stubEdges = ctx.graph.edges.map((edge) => ({
      id: `${edge.from}::${edge.to}::${edge.relation}`,
      fromId: edge.from,
      toId: edge.to,
      relation: edge.relation,
    }));

    let cancelled = false;
    void runElkLayout(stubNodes, stubEdges).then((nextPositions) => {
      if (!cancelled) {
        setPositions(nextPositions);
      }
    });

    return () => {
      cancelled = true;
    };
  }, [ctx.graph.edges, ctx.graph.nodes, topologyKey]);

  useEffect(() => {
    rendererRef.current?.updateGraph(renderGraph);
  }, [renderGraph, rendererRef]);

  return (
    <div
      className="explorer-canvas"
      aria-label="Codebase graph"
      style={{ width: "100%", height: "100%", position: "relative" }}
    >
      <canvas
        ref={canvasRef}
        style={{ display: "block", width: "100%", height: "100%" }}
      />
      <ContextMenu
        nodeId={contextMenu?.nodeId ?? null}
        x={contextMenu?.x ?? 0}
        y={contextMenu?.y ?? 0}
        onClose={() => setContextMenu(null)}
      />
    </div>
  );
}
