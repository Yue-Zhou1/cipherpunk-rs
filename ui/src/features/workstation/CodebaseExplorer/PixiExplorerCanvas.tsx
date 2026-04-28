import { useCallback, useEffect, useRef, useState } from "react";
import { AnimatePresence } from "framer-motion";

import { ContextMenu } from "./ContextMenu";
import { useExplorer } from "./ExplorerContext";
import { useRenderGraph } from "./hooks/useRenderGraph";
import { useTraceAlgorithm } from "./hooks/useTraceAlgorithm";
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
  const { traceResult, traceToEntryPoint, traceDataflowForNode, clearTrace } =
    useTraceAlgorithm(ctx.graph, ctx.nodeMap);
  const renderGraph = useRenderGraph(ctx, positions, traceResult);

  useEffect(() => {
    setContextMenu(null);
    clearTrace();
  }, [clearTrace, ctx.sessionId]);

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
    if (traceResult) {
      clearTrace();
    } else if (ctx.neighborhoodResult) {
      ctx.clearHighlight();
    } else if (ctx.stateKind === "focus") {
      ctx.clearFocus();
    }
  }, [clearTrace, ctx, traceResult]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") {
        return;
      }
      setContextMenu(null);
      if (traceResult) {
        clearTrace();
      } else if (ctx.neighborhoodResult) {
        ctx.clearHighlight();
      } else if (ctx.stateKind === "focus") {
        ctx.clearFocus();
      }
    };

    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [clearTrace, ctx, traceResult]);

  const { rendererRef, initError } = usePixiRenderer(canvasRef, {
    onNodeClick: handleNodeClick,
    onNodeRightClick: handleNodeRightClick,
    onPaneClick: handlePaneClick,
  });

  useEffect(() => {
    const canvas = canvasRef.current;
    const container = canvas?.parentElement;
    if (!container) {
      return;
    }

    const applySize = (width: number, height: number) => {
      setCanvasSize({
        width: width > 0 ? width : 1200,
        height: height > 0 ? height : 800,
      });
    };

    const initialRect = container.getBoundingClientRect();
    applySize(initialRect.width, initialRect.height);

    const observer = new ResizeObserver((entries) => {
      const entry = entries[0];
      if (!entry) {
        return;
      }
      applySize(entry.contentRect.width, entry.contentRect.height);
    });
    observer.observe(container);

    return () => {
      observer.disconnect();
    };
  }, []);

  useEffect(() => {
    rendererRef.current?.updateGraph(renderGraph);
  }, [renderGraph, rendererRef]);

  useEffect(() => {
    const renderer = rendererRef.current;
    if (!renderer) {
      return;
    }

    if (traceResult) {
      const pathEdges = renderGraph.edges.filter((edge) =>
        traceResult.pathEdgeIds.has(edge.id)
      );
      renderer.setParticleEdges(pathEdges);
    } else {
      renderer.setParticleEdges([]);
    }
  }, [renderGraph, rendererRef, traceResult]);

  return (
    <div
      className="explorer-canvas"
      aria-label="Codebase graph"
      style={{ width: "100%", height: "100%", position: "relative" }}
    >
      {traceResult ? (
        <div className="explorer-trace-banner" role="status">
          <span>{traceResult.banner}</span>
          <button
            type="button"
            onClick={clearTrace}
            aria-label="Clear trace"
            className="explorer-trace-close"
          >
            ✕
          </button>
        </div>
      ) : null}
      {initError ? (
        <div className="explorer-canvas-error" role="alert">
          {initError}
        </div>
      ) : null}
      <canvas
        ref={canvasRef}
        style={{ display: "block", width: "100%", height: "100%" }}
      />
      <AnimatePresence>
        {contextMenu ? (
          <ContextMenu
            nodeId={contextMenu.nodeId}
            x={contextMenu.x}
            y={contextMenu.y}
            onClose={() => setContextMenu(null)}
            traceCtx={{
              traceToEntryPoint,
              traceDataflowForNode,
            }}
          />
        ) : null}
      </AnimatePresence>
    </div>
  );
}
