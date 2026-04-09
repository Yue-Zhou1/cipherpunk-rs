import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import ReactFlow, {
  Background,
  Controls,
  MiniMap,
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
import { NodeContextMenu } from "./NodeContextMenu";

const elk = new ELK();

const nodeTypes = {
  clusterNode: ClusterNode,
  fileNode: FileNode,
  symbolNode: SymbolNode,
};

async function layoutWithElk(
  nodes: Node[],
  edges: Array<{ id: string; source: string; target: string }>
): Promise<Node[]> {
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

  return nodes.map((node) => ({ ...node, position: positions.get(node.id) ?? { x: 0, y: 0 } }));
}

export function ExplorerCanvas() {
  const ctx = useExplorer();
  const [positionByNodeId, setPositionByNodeId] = useState<Map<string, { x: number; y: number }>>(
    new Map()
  );
  const [contextMenu, setContextMenu] = useState<{
    x: number;
    y: number;
    nodeId: string;
  } | null>(null);
  const flowRef = useRef<ReactFlowInstance | null>(null);

  const flowModel = useMemo(
    () =>
      buildFlowModel(ctx.graph, {
        resolvedGranularity: ctx.resolvedGranularity,
        expandedClusters: ctx.expandedClusters,
        focusedNodeId: ctx.focusedNodeId,
        upstreamIds: ctx.upstreamIds,
        downstreamIds: ctx.downstreamIds,
        neighborhoodResult: ctx.neighborhoodResult,
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
      ctx.neighborhoodResult,
      ctx.matchingNodeIds,
      ctx.stateKind,
    ]
  );

  const topologyKey = useMemo(() => {
    const nodeIds = flowModel.nodes.map((node) => node.id).sort().join(",");
    const edgeIds = flowModel.edges.map((edge) => edge.id).sort().join(",");
    return `${nodeIds}|${edgeIds}`;
  }, [flowModel.edges, flowModel.nodes]);

  const renderedNodes = useMemo(
    () =>
      flowModel.nodes.map((node, index) => ({
        ...node,
        position:
          positionByNodeId.get(node.id) ?? {
            x: (index % 5) * 320,
            y: Math.floor(index / 5) * 120,
          },
      })),
    [flowModel.nodes, positionByNodeId]
  );

  useEffect(() => {
    void layoutWithElk(flowModel.nodes, flowModel.edges)
      .then((positionedNodes) => {
        setPositionByNodeId(new Map(positionedNodes.map((node) => [node.id, node.position])));
        requestAnimationFrame(() => {
          flowRef.current?.fitView?.({ padding: 0.16 });
        });
      })
      .catch(() => {
        setPositionByNodeId(
          new Map(
            flowModel.nodes.map((node, index) => [
              node.id,
              { x: (index % 5) * 320, y: Math.floor(index / 5) * 120 },
            ])
          )
        );
        requestAnimationFrame(() => {
          flowRef.current?.fitView?.({ padding: 0.16 });
        });
      });
  }, [topologyKey]);

  const handleNodeClick = useCallback(
    (_event: React.MouseEvent, node: Node) => {
      setContextMenu(null);
      if (node.type === "clusterNode") {
        ctx.expandCluster(node.id);
        ctx.toggleCluster(node.id);
        return;
      }
      ctx.focusNode(node.id);
    },
    [ctx]
  );

  const handleNodeContextMenu = useCallback(
    (event: React.MouseEvent, node: Node) => {
      event.preventDefault();
      ctx.focusNode(node.id);
      setContextMenu({
        x: event.clientX,
        y: event.clientY,
        nodeId: node.id,
      });
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
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [ctx]);

  return (
    <div className="explorer-canvas" aria-label="Codebase graph">
      <ReactFlow
        nodes={renderedNodes}
        edges={flowModel.edges}
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
        onNodeContextMenu={handleNodeContextMenu}
        onPaneClick={handlePaneClick}
      >
        <Background color="#2f3845" gap={20} size={1} />
        <Controls position="top-right" />
        <MiniMap position="bottom-right" zoomable pannable />
      </ReactFlow>
      {contextMenu ? (
        <NodeContextMenu
          x={contextMenu.x}
          y={contextMenu.y}
          nodeId={contextMenu.nodeId}
          hasFilePath={!!ctx.nodeMap.get(contextMenu.nodeId)?.filePath}
          isFocused={ctx.focusedNodeId === contextMenu.nodeId}
          onShowCallers={() => {
            ctx.showCallers();
            setContextMenu(null);
          }}
          onShowCallees={() => {
            ctx.showCallees();
            setContextMenu(null);
          }}
          onOpenInEditor={() => {
            const node = ctx.nodeMap.get(contextMenu.nodeId);
            if (node?.filePath) {
              ctx.onNavigateToSource?.(node.filePath, node.line);
            }
            setContextMenu(null);
          }}
          onToggleFocus={() => {
            if (ctx.focusedNodeId === contextMenu.nodeId) {
              ctx.clearFocus();
            } else {
              ctx.focusNode(contextMenu.nodeId);
            }
            setContextMenu(null);
          }}
          onClose={() => setContextMenu(null)}
        />
      ) : null}
    </div>
  );
}
