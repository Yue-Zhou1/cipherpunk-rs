import { useCallback, useEffect, useRef, useState } from "react";

import { ContextMenu } from "./ContextMenu";
import { useExplorer } from "./ExplorerContext";
import { useRenderGraph } from "./hooks/useRenderGraph";
import { useLayout } from "./layout/useLayout";
import { usePixiRenderer } from "./pixi/usePixiRenderer";

export function PixiExplorerCanvas() {
  const ctx = useExplorer();
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [canvasSize, setCanvasSize] = useState({ width: 1200, height: 800 });
  const [contextMenu, setContextMenu] = useState<{
    nodeId: string;
    x: number;
    y: number;
  } | null>(null);
  const positions = useLayout({
    graph: ctx.graph,
    focusedNodeId: ctx.focusedNodeId,
    upstreamIds: ctx.upstreamIds,
    downstreamIds: ctx.downstreamIds,
    canvasSize,
  });
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

  useEffect(() => {
    const updateCanvasSize = () => {
      const canvas = canvasRef.current;
      const rect = canvas?.parentElement?.getBoundingClientRect();
      if (rect) {
        setCanvasSize({
          width: rect.width > 0 ? rect.width : 1200,
          height: rect.height > 0 ? rect.height : 800,
        });
      }
    };

    updateCanvasSize();
    window.addEventListener("resize", updateCanvasSize);
    return () => {
      window.removeEventListener("resize", updateCanvasSize);
    };
  }, []);

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
