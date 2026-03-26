import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import ReactFlow, {
  Background,
  Controls,
  MiniMap,
  type Edge,
  type Node,
  type ReactFlowInstance,
} from "reactflow";
import ELK from "elkjs/lib/elk.bundled.js";
import "reactflow/dist/style.css";

import { buildFlowModel } from "./AdaptiveLayout";
import { useExplorer } from "./ExplorerContext";
import { ClusterNode } from "./nodes/ClusterNode";
import { FileNode } from "./nodes/FileNode";
import { SymbolNode } from "./nodes/SymbolNode";

const elk = new ELK();

const nodeTypes = {
  clusterNode: ClusterNode,
  fileNode: FileNode,
  symbolNode: SymbolNode,
};

async function layoutWithElk(nodes: Node[], edges: Edge[]): Promise<{ nodes: Node[]; edges: Edge[] }> {
  const layout = await elk.layout({
    id: "root",
    layoutOptions: {
      "elk.algorithm": "layered",
      "elk.direction": "DOWN",
      "elk.spacing.nodeNode": "36",
      "elk.layered.spacing.nodeNodeBetweenLayers": "72",
      "elk.layered.nodePlacement.strategy": "NETWORK_SIMPLEX",
    },
    children: nodes.map((node) => ({
      id: node.id,
      width: node.type === "symbolNode" ? 280 : node.type === "clusterNode" ? 220 : 168,
      height: node.type === "symbolNode" ? 80 : 48,
    })),
    edges: edges.map((edge) => ({
      id: edge.id,
      sources: [edge.source],
      targets: [edge.target],
    })),
  });

  const positions = new Map(
    (layout.children ?? []).map((child) => [child.id, { x: child.x ?? 0, y: child.y ?? 0 }])
  );

  return {
    nodes: nodes.map((node) => ({ ...node, position: positions.get(node.id) ?? { x: 0, y: 0 } })),
    edges,
  };
}

export function ExplorerCanvas() {
  const ctx = useExplorer();
  const [nodes, setNodes] = useState<Node[]>([]);
  const [edges, setEdges] = useState<Edge[]>([]);
  const flowRef = useRef<ReactFlowInstance | null>(null);

  const tracePathIds = useMemo(
    () => (ctx.traceResult ? new Set(ctx.traceResult.path) : null),
    [ctx.traceResult]
  );

  const flowModel = useMemo(
    () =>
      buildFlowModel(ctx.graph, {
        resolvedGranularity: ctx.resolvedGranularity,
        expandedClusters: ctx.expandedClusters,
        focusedNodeId: ctx.focusedNodeId,
        upstreamIds: ctx.upstreamIds,
        downstreamIds: ctx.downstreamIds,
        tracePathIds,
        matchingNodeIds: ctx.matchingNodeIds,
        stateKind: ctx.stateKind,
      }),
    [
      ctx.graph,
      ctx.resolvedGranularity,
      ctx.expandedClusters,
      ctx.focusedNodeId,
      ctx.upstreamIds,
      ctx.downstreamIds,
      tracePathIds,
      ctx.matchingNodeIds,
      ctx.stateKind,
    ]
  );

  const nodesWithCallbacks = useMemo(
    () =>
      flowModel.nodes.map((node) => {
        if (node.type !== "symbolNode") {
          return node;
        }

        return {
          ...node,
          data: {
            ...node.data,
            onParameterClick: ctx.traceParameter,
            onReturnClick: ctx.traceReturn,
          },
        };
      }),
    [ctx.traceParameter, ctx.traceReturn, flowModel.nodes]
  );

  useEffect(() => {
    void layoutWithElk(nodesWithCallbacks, flowModel.edges)
      .then((result) => {
        setNodes(result.nodes);
        setEdges(result.edges);
      })
      .catch(() => {
        setNodes(
          nodesWithCallbacks.map((node, index) => ({
            ...node,
            position: { x: (index % 5) * 320, y: Math.floor(index / 5) * 120 },
          }))
        );
        setEdges(flowModel.edges);
      });
  }, [flowModel.edges, nodesWithCallbacks]);

  const handleNodeClick = useCallback(
    (_event: React.MouseEvent, node: Node) => {
      if (node.type === "clusterNode") {
        ctx.toggleCluster(node.id);
        return;
      }
      ctx.focusNode(node.id);
    },
    [ctx]
  );

  const handlePaneClick = useCallback(() => {
    if (ctx.stateKind === "trace") {
      ctx.clearTrace();
      return;
    }
    if (ctx.stateKind === "focus") {
      ctx.clearFocus();
    }
  }, [ctx]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") {
        return;
      }
      if (ctx.stateKind === "trace") {
        ctx.clearTrace();
      } else if (ctx.stateKind === "focus") {
        ctx.clearFocus();
      }
    };

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [ctx]);

  return (
    <div className="explorer-canvas" aria-label="Codebase graph">
      <ReactFlow
        nodes={nodes}
        edges={edges}
        nodeTypes={nodeTypes}
        fitView
        fitViewOptions={{ padding: 0.16 }}
        minZoom={0.1}
        maxZoom={3}
        proOptions={{ hideAttribution: true }}
        onInit={(instance) => {
          flowRef.current = instance;
        }}
        onNodeClick={handleNodeClick}
        onPaneClick={handlePaneClick}
      >
        <Background color="#2f3845" gap={20} size={1} />
        <Controls position="top-right" />
        <MiniMap position="bottom-right" zoomable pannable />
      </ReactFlow>
    </div>
  );
}
