import { useEffect, useMemo, useRef, useState } from "react";
import ELK from "elkjs/lib/elk.bundled.js";
import ReactFlow, {
  Background,
  Controls,
  MarkerType,
  MiniMap,
  Position,
  type NodeProps,
  type ReactFlowInstance,
  type Edge,
  type Node,
} from "reactflow";

import {
  loadDataflowGraph,
  loadFeatureGraph,
  loadFileGraph,
  loadSymbolGraph,
  type GraphLensKind,
  type ProjectGraphNode,
  type ProjectGraphResponse,
} from "../../ipc/commands";
import type { GraphLensProps } from "./GraphLensCytoscape";
import "reactflow/dist/style.css";

const LENS_OPTIONS: Array<{ kind: GraphLensKind; label: string }> = [
  { kind: "file", label: "File Graph" },
  { kind: "feature", label: "Feature Graph" },
  { kind: "dataflow", label: "Dataflow Graph" },
  { kind: "symbol", label: "Symbol Graph" },
];
const elk = new ELK();

type FlowNodeData = {
  label: string;
  kind: string;
  filePath?: string;
  moduleKey?: string;
  isModule: boolean;
  collapsed?: boolean;
  line?: number;
  findingCount?: number;
  maxSeverity?: string;
};

type FlowEdgeData = {
  relation: string;
  valuePreview?: string;
};

type FlowNode = Node<FlowNodeData>;
type FlowEdge = Edge<FlowEdgeData>;

type FlowModel = {
  nodes: FlowNode[];
  edges: FlowEdge[];
};

type HoveredNodeState = {
  x: number;
  y: number;
  data: FlowNodeData;
};

function GraphPlaceholder({
  title,
  detail,
}: {
  title: string;
  detail: string;
}): JSX.Element {
  return (
    <div className="flex items-center justify-center h-full text-gray-500">
      <div className="text-center">
        <p className="text-lg font-medium">{title}</p>
        <p className="text-sm mt-1">{detail}</p>
      </div>
    </div>
  );
}

function isJsdomRuntime(): boolean {
  return typeof navigator !== "undefined" && navigator.userAgent.toLowerCase().includes("jsdom");
}

function normalizePath(path: string): string {
  return path.replaceAll("\\", "/").replace(/\/+/g, "/").replace(/^\.\//, "");
}

function moduleKeyForNode(node: ProjectGraphNode): string | null {
  if (!node.filePath) {
    return null;
  }

  const normalized = normalizePath(node.filePath);
  const slashIndex = normalized.lastIndexOf("/");
  if (slashIndex <= 0) {
    return null;
  }

  return normalized.slice(0, slashIndex);
}

function moduleId(moduleKey: string): string {
  return `module:${moduleKey}`;
}

function moduleLabel(moduleKey: string): string {
  const segments = moduleKey.split("/").filter((segment) => segment.length > 0);
  return segments[segments.length - 1] ?? moduleKey;
}

function parseLineFromNode(node: ProjectGraphNode): number | undefined {
  if (typeof node.line === "number" && Number.isInteger(node.line) && node.line > 0) {
    return node.line;
  }
  const segments = node.id.split(":").reverse();
  for (const segment of segments) {
    if (!/^\d+$/.test(segment)) {
      continue;
    }
    const parsed = Number(segment);
    if (Number.isInteger(parsed) && parsed > 0) {
      return parsed;
    }
  }
  return undefined;
}

function normalizedSeverity(severity?: string): string | undefined {
  if (!severity) {
    return undefined;
  }
  const normalized = severity.trim().toLowerCase();
  if (["critical", "high", "medium", "low", "observation"].includes(normalized)) {
    return normalized;
  }
  return undefined;
}

function GraphNodeCard({ data }: NodeProps<FlowNodeData>): JSX.Element {
  const severity = normalizedSeverity(data.maxSeverity);
  const hasFindings = !!data.findingCount && data.findingCount > 0;

  return (
    <div className="graph-flow-node-content">
      <span className="graph-flow-node-label">{data.label}</span>
      {hasFindings ? (
        <span
          className={`graph-flow-node-badge severity-${severity ?? "low"}`}
          aria-label={`${data.findingCount} findings`}
        >
          {data.findingCount}
        </span>
      ) : null}
    </div>
  );
}

function nodeSize(node: FlowNode): { width: number; height: number } {
  if (node.data.isModule) {
    return { width: 220, height: 64 };
  }

  const width = Math.min(300, Math.max(168, node.data.label.length * 6 + 48));
  return { width, height: 56 };
}

async function layoutWithElk(nodes: FlowNode[], edges: FlowEdge[]): Promise<FlowModel> {
  const layout = await elk.layout({
    id: "root",
    layoutOptions: {
      "elk.algorithm": "layered",
      "elk.direction": "DOWN",
      "elk.spacing.nodeNode": "36",
      "elk.layered.spacing.nodeNodeBetweenLayers": "72",
      "elk.layered.nodePlacement.strategy": "NETWORK_SIMPLEX",
      "elk.layered.crossingMinimization.strategy": "LAYER_SWEEP",
    },
    children: nodes.map((node) => {
      const size = nodeSize(node);
      return {
        id: node.id,
        width: size.width,
        height: size.height,
      };
    }),
    edges: edges.map((edge) => ({
      id: edge.id,
      sources: [edge.source],
      targets: [edge.target],
    })),
  });

  const positionedById = new Map(
    (layout.children ?? []).map((child) => [child.id, { x: child.x ?? 0, y: child.y ?? 0 }])
  );

  return {
    nodes: nodes.map((node) => ({
      ...node,
      position: positionedById.get(node.id) ?? { x: 0, y: 0 },
    })),
    edges,
  };
}

function gridLayoutFallback(nodes: FlowNode[], edges: FlowEdge[]): FlowModel {
  return {
    nodes: nodes.map((node, index) => {
      const column = index % 4;
      const row = Math.floor(index / 4);
      return {
        ...node,
        position: {
          x: column * 320,
          y: row * 100,
        },
      };
    }),
    edges,
  };
}

function buildFlowModel(
  graph: ProjectGraphResponse,
  collapsedModules: Set<string>,
  selectedNodeIds: Set<string>,
  focusedSymbol: string,
  matchingNodeIds: Set<string> | null
): FlowModel {
  const nodes: FlowNode[] = [];
  const edges: FlowEdge[] = [];
  const moduleByNodeId = new Map<string, string>();
  const moduleKeys = new Set<string>();
  const nodeById = new Map(graph.nodes.map((node) => [node.id, node]));

  for (const node of graph.nodes) {
    const moduleKey = moduleKeyForNode(node);
    if (!moduleKey) {
      continue;
    }
    moduleByNodeId.set(node.id, moduleKey);
    moduleKeys.add(moduleKey);
  }

  const sortedModuleKeys = Array.from(moduleKeys).sort((left, right) => left.localeCompare(right));
  for (const moduleKey of sortedModuleKeys) {
    const collapsed = collapsedModules.has(moduleKey);
    nodes.push({
      id: moduleId(moduleKey),
      type: "graphNode",
      position: { x: 0, y: 0 },
      sourcePosition: Position.Bottom,
      targetPosition: Position.Top,
      className: `graph-flow-node module${collapsed ? " collapsed" : ""}`,
      style: {
        width: 220,
        height: 64,
      },
      data: {
        label: moduleLabel(moduleKey),
        kind: "module",
        moduleKey,
        isModule: true,
        collapsed,
      },
    });
  }

  for (const sourceNode of graph.nodes) {
    const moduleKey = moduleByNodeId.get(sourceNode.id);
    if (moduleKey && collapsedModules.has(moduleKey)) {
      continue;
    }

    const normalizedFilePath = sourceNode.filePath ? normalizePath(sourceNode.filePath) : undefined;
    const selectedBySymbol =
      focusedSymbol.length > 0 && sourceNode.label.toLowerCase().includes(focusedSymbol);
    const selected = selectedNodeIds.has(sourceNode.id) || selectedBySymbol;
    const dimmed = matchingNodeIds ? !matchingNodeIds.has(sourceNode.id) : false;
    const severity = normalizedSeverity(sourceNode.maxSeverity);
    const className = [
      "graph-flow-node",
      selected ? "selected" : "",
      dimmed ? "search-dimmed" : "",
      severity ? `severity-${severity}` : "",
      sourceNode.findingCount && sourceNode.findingCount > 0 ? "has-findings" : "",
    ]
      .filter((value) => value.length > 0)
      .join(" ");

    nodes.push({
      id: sourceNode.id,
      type: "graphNode",
      position: { x: 0, y: 0 },
      sourcePosition: Position.Bottom,
      targetPosition: Position.Top,
      className,
      style: dimmed ? { opacity: 0.15 } : undefined,
      data: {
        label: sourceNode.label,
        kind: sourceNode.kind,
        filePath: normalizedFilePath,
        moduleKey,
        isModule: false,
        line: parseLineFromNode(sourceNode),
        findingCount: sourceNode.findingCount,
        maxSeverity: severity,
      },
    });
  }

  const visibleNodeIds = new Set(nodes.map((node) => node.id));
  const remapNodeId = (nodeId: string): string => {
    const sourceNode = nodeById.get(nodeId);
    if (!sourceNode) {
      return nodeId;
    }
    const moduleKey = moduleByNodeId.get(sourceNode.id);
    if (moduleKey && collapsedModules.has(moduleKey)) {
      return moduleId(moduleKey);
    }
    return nodeId;
  };

  const edgeKeys = new Set<string>();
  for (const sourceEdge of graph.edges) {
    const source = remapNodeId(sourceEdge.from);
    const target = remapNodeId(sourceEdge.to);
    if (source === target || !visibleNodeIds.has(source) || !visibleNodeIds.has(target)) {
      continue;
    }

    const edgeKey = `${source}::${target}::${sourceEdge.relation}::${sourceEdge.valuePreview ?? ""}`;
    if (edgeKeys.has(edgeKey)) {
      continue;
    }
    edgeKeys.add(edgeKey);

    const label = sourceEdge.valuePreview
      ? `${sourceEdge.relation} (${sourceEdge.valuePreview})`
      : sourceEdge.relation;
    edges.push({
      id: edgeKey,
      source,
      target,
      label,
      type: "smoothstep",
      animated: sourceEdge.relation === "parameter_flow" || sourceEdge.relation === "return_flow",
      markerEnd: {
        type: MarkerType.ArrowClosed,
        width: 18,
        height: 18,
        color: "#5c7394",
      },
      data: {
        relation: sourceEdge.relation,
        valuePreview: sourceEdge.valuePreview,
      },
      style: {
        stroke: "#5c7394",
        strokeWidth: 1.4,
      },
    });
  }

  return { nodes, edges };
}

function displayTitle(lens: GraphLensKind): string {
  return LENS_OPTIONS.find((entry) => entry.kind === lens)?.label ?? "Graph Lens";
}

function nodeColor(node: FlowNode): string {
  if (node.data.isModule) {
    return node.data.collapsed ? "#7d6440" : "#4b5f78";
  }
  if (node.className?.includes("selected")) {
    return "#8f7d3d";
  }
  if (node.data.kind === "feature") {
    return "#4b6a8a";
  }
  if (node.data.kind === "dataflow") {
    return "#2d7d6d";
  }
  return "#37567f";
}

function GraphLensReactFlow({
  sessionId,
  selectedNodeIds = [],
  onNavigateToSource,
  focusSymbolName,
}: GraphLensProps): JSX.Element {
  const [lens, setLens] = useState<GraphLensKind>("file");
  const [includeValues, setIncludeValues] = useState(false);
  const [graph, setGraph] = useState<ProjectGraphResponse | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState("");
  const [collapsedModules, setCollapsedModules] = useState<string[]>([]);
  const [nodes, setNodes] = useState<FlowNode[]>([]);
  const [edges, setEdges] = useState<FlowEdge[]>([]);
  const [isLayouting, setIsLayouting] = useState(false);
  const [layoutFallback, setLayoutFallback] = useState(false);
  const [hoveredNode, setHoveredNode] = useState<HoveredNodeState | null>(null);
  const [selectedEdge, setSelectedEdge] = useState<FlowEdge | null>(null);
  const layoutRequestRef = useRef(0);
  const flowInstanceRef = useRef<ReactFlowInstance | null>(null);

  const jsdom = useMemo(() => isJsdomRuntime(), []);
  const nodeTypes = useMemo(() => ({ graphNode: GraphNodeCard }), []);
  const collapsedModuleSet = useMemo(() => new Set(collapsedModules), [collapsedModules]);
  const sortedSelectedNodeIds = useMemo(
    () => [...selectedNodeIds].sort((left, right) => left.localeCompare(right)),
    [selectedNodeIds]
  );
  const selectedNodeSet = useMemo(() => new Set(sortedSelectedNodeIds), [sortedSelectedNodeIds]);
  const selectedNodeKey = sortedSelectedNodeIds.join("|");
  const focusedSymbol = (focusSymbolName ?? "").trim().toLowerCase();
  const matchingNodeIds = useMemo(() => {
    if (!graph || !searchQuery.trim()) {
      return null;
    }
    const query = searchQuery.trim().toLowerCase();
    return new Set(
      graph.nodes
        .filter(
          (node) =>
            node.label.toLowerCase().includes(query) || node.id.toLowerCase().includes(query)
        )
        .map((node) => node.id)
    );
  }, [graph, searchQuery]);

  useEffect(() => {
    let cancelled = false;
    setIsLoading(true);
    setError(null);
    setSelectedEdge(null);

    const request =
      lens === "file"
        ? loadFileGraph(sessionId)
        : lens === "feature"
          ? loadFeatureGraph(sessionId)
          : lens === "dataflow"
            ? loadDataflowGraph(sessionId, includeValues)
            : loadSymbolGraph(sessionId);

    void request
      .then((response) => {
        if (cancelled) {
          return;
        }
        setGraph(response);
        setCollapsedModules([]);
        setLayoutFallback(false);
      })
      .catch(() => {
        if (!cancelled) {
          setError("Unable to load graph lens.");
          setGraph(null);
          setNodes([]);
          setEdges([]);
          setLayoutFallback(false);
        }
      })
      .finally(() => {
        if (!cancelled) {
          setIsLoading(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [includeValues, lens, sessionId]);

  useEffect(() => {
    if (lens !== "dataflow" && includeValues) {
      setIncludeValues(false);
    }
  }, [includeValues, lens]);

  useEffect(() => {
    if (!graph) {
      setNodes([]);
      setEdges([]);
      setLayoutFallback(false);
      return;
    }

    const model = buildFlowModel(
      graph,
      collapsedModuleSet,
      selectedNodeSet,
      focusedSymbol,
      matchingNodeIds
    );
    if (jsdom) {
      setNodes(model.nodes);
      setEdges(model.edges);
      setIsLayouting(false);
      setLayoutFallback(false);
      return;
    }

    let cancelled = false;
    const requestId = layoutRequestRef.current + 1;
    layoutRequestRef.current = requestId;
    setIsLayouting(true);

    void layoutWithElk(model.nodes, model.edges)
      .then((layouted) => {
        if (cancelled || layoutRequestRef.current !== requestId) {
          return;
        }
        setNodes(layouted.nodes);
        setEdges(layouted.edges);
        setLayoutFallback(false);
      })
      .catch((layoutError) => {
        if (!cancelled && layoutRequestRef.current === requestId) {
          console.warn("ELK layout failed, using grid fallback:", layoutError);
          const fallback = gridLayoutFallback(model.nodes, model.edges);
          setNodes(fallback.nodes);
          setEdges(fallback.edges);
          setLayoutFallback(true);
        }
      })
      .finally(() => {
        if (!cancelled && layoutRequestRef.current === requestId) {
          setIsLayouting(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [
    collapsedModuleSet,
    focusedSymbol,
    graph,
    jsdom,
    matchingNodeIds,
    selectedNodeKey,
    selectedNodeSet,
  ]);

  const title = displayTitle(lens);

  const handleNodeClick = (_event: React.MouseEvent, node: FlowNode): void => {
    if (node.data.isModule && node.data.moduleKey) {
      const target = node.data.moduleKey;
      setCollapsedModules((previous) => {
        if (previous.includes(target)) {
          return previous.filter((value) => value !== target);
        }
        return [...previous, target].sort((left, right) => left.localeCompare(right));
      });
      return;
    }

    if (node.data.filePath && onNavigateToSource) {
      onNavigateToSource(node.data.filePath, node.data.line);
    }
  };

  const handleNodeHover = (event: React.MouseEvent, node: FlowNode): void => {
    setHoveredNode({
      x: event.clientX,
      y: event.clientY,
      data: node.data,
    });
  };

  const fitToScreen = (): void => {
    if (!flowInstanceRef.current) {
      return;
    }
    if (matchingNodeIds && matchingNodeIds.size > 0) {
      const matching = nodes.filter((node) => matchingNodeIds.has(node.id));
      if (matching.length > 0) {
        flowInstanceRef.current.fitView({ nodes: matching, padding: 0.2, duration: 180 });
        return;
      }
    }
    flowInstanceRef.current.fitView({ padding: 0.16, duration: 180 });
  };

  const layoutSummary = `${nodes.length} nodes / ${edges.length} edges`;

  return (
    <section className="panel workstation-graph-lens" aria-label="Graph Lens">
      <div className="workstation-panel-head">
        <p className="workstation-panel-eyebrow">Graph Lens</p>
        <h2>{title}</h2>
      </div>

      <div className="graph-lens-toolbar" role="tablist" aria-label="Graph lens selector">
        <select
          value={lens}
          onChange={(event) => setLens(event.target.value as GraphLensKind)}
          className="graph-lens-select"
          aria-label="Select graph lens"
        >
          {LENS_OPTIONS.map((entry) => (
            <option key={entry.kind} value={entry.kind}>
              {entry.label}
            </option>
          ))}
        </select>
        <input
          type="text"
          placeholder="Search..."
          value={searchQuery}
          onChange={(event) => setSearchQuery(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              fitToScreen();
            }
          }}
          className="graph-lens-search"
        />
        {lens === "dataflow" ? (
          <label className="graph-lens-toggle">
            <input
              type="checkbox"
              checked={includeValues}
              onChange={(event) => setIncludeValues(event.target.checked)}
            />
            <span>Values</span>
          </label>
        ) : null}
        {matchingNodeIds ? (
          <span className="muted-text">
            {matchingNodeIds.size} matches
          </span>
        ) : null}
        <button type="button" className="graph-lens-fit-button" onClick={fitToScreen}>
          Fit
        </button>
      </div>

      {isLoading ? <p className="muted-text">Loading graph...</p> : null}
      {selectedNodeIds.length > 0 ? (
        <p className="muted-text">Review context selected {selectedNodeIds.length} node(s).</p>
      ) : null}

      {!isLoading && error && !graph ? (
        <GraphPlaceholder
          title="No graph data available"
          detail="Run the BuildProjectIr job to generate the code graph."
        />
      ) : null}

      {!isLoading && !error && graph && graph.nodes.length === 0 ? (
        <GraphPlaceholder
          title="Graph is empty"
          detail="Graph is empty - no source files found in the selected scope."
        />
      ) : null}

      {!isLoading && !error && graph && graph.nodes.length > 0 ? (
        <>
          <p className="muted-text">
            {layoutSummary}
            {isLayouting ? " (layouting...)" : ""}
            {layoutFallback ? " | Using simplified layout" : ""}
          </p>

          {jsdom ? (
            <div className="graph-lens-grid">
              <div className="graph-lens-block">
                <h3>Nodes</h3>
                <ul>
                  {nodes.slice(0, 32).map((node) => (
                    <li
                      key={node.id}
                      data-testid="graph-node-row"
                      className={node.className || undefined}
                    >
                      {node.data.filePath && onNavigateToSource ? (
                        <button
                          type="button"
                          className="graph-node-link"
                          onClick={() => onNavigateToSource(node.data.filePath!, node.data.line)}
                        >
                          <code>{node.data.label}</code>
                        </button>
                      ) : (
                        <code>{node.data.label}</code>
                      )}
                      {node.data.findingCount && node.data.findingCount > 0 ? (
                        <span
                          data-testid="graph-node-finding-badge"
                          className={`graph-node-finding-badge severity-${normalizedSeverity(node.data.maxSeverity) ?? "low"}`}
                        >
                          {node.data.findingCount}
                        </span>
                      ) : null}
                    </li>
                  ))}
                </ul>
              </div>
              <div className="graph-lens-block">
                <h3>Edges</h3>
                <ul>
                  {edges.slice(0, 8).map((edge) => (
                    <li key={edge.id}>
                      <code>
                        {edge.data?.relation}: {edge.source} -&gt; {edge.target}
                      </code>
                      {edge.data?.valuePreview ? <span> ({edge.data.valuePreview})</span> : null}
                    </li>
                  ))}
                </ul>
              </div>
            </div>
          ) : (
            <div className="graph-lens-canvas graph-lens-reactflow-canvas">
              <ReactFlow
                nodes={nodes}
                edges={edges}
                nodeTypes={nodeTypes}
                fitView
                fitViewOptions={{ padding: 0.16 }}
                minZoom={0.15}
                maxZoom={2.8}
                proOptions={{ hideAttribution: true }}
                onInit={(instance) => {
                  flowInstanceRef.current = instance;
                }}
                onPaneClick={() => {
                  setHoveredNode(null);
                  setSelectedEdge(null);
                }}
                onNodeClick={handleNodeClick}
                onNodeMouseEnter={handleNodeHover}
                onNodeMouseMove={handleNodeHover}
                onNodeMouseLeave={() => setHoveredNode(null)}
                onEdgeClick={(_event, edge) => setSelectedEdge(edge)}
              >
                <Background color="#2f3845" gap={20} size={1} />
                <Controls position="top-right" />
                <MiniMap
                  position="bottom-right"
                  zoomable
                  pannable
                  nodeColor={(node) => nodeColor(node as FlowNode)}
                />
              </ReactFlow>
            </div>
          )}

          {selectedEdge ? (
            <div className="graph-lens-edge-detail" role="status" aria-live="polite">
              <p className="graph-lens-edge-title">Edge Detail</p>
              <p className="muted-text">
                <strong>{selectedEdge.data?.relation}</strong>: {selectedEdge.source} -&gt; {selectedEdge.target}
              </p>
              {selectedEdge.data?.valuePreview ? (
                <p className="muted-text">Value preview: {selectedEdge.data.valuePreview}</p>
              ) : null}
            </div>
          ) : null}

          {hoveredNode ? (
            <aside
              className="graph-lens-tooltip"
              style={{ left: hoveredNode.x + 12, top: hoveredNode.y + 12 }}
            >
              <p className="graph-lens-tooltip-title">{hoveredNode.data.label}</p>
              <p className="graph-lens-tooltip-line">kind: {hoveredNode.data.kind}</p>
              {hoveredNode.data.filePath ? (
                <p className="graph-lens-tooltip-line">file: {hoveredNode.data.filePath}</p>
              ) : null}
              {hoveredNode.data.line ? (
                <p className="graph-lens-tooltip-line">line: {hoveredNode.data.line}</p>
              ) : null}
              {hoveredNode.data.isModule ? (
                <p className="graph-lens-tooltip-line">
                  click to {hoveredNode.data.collapsed ? "expand" : "collapse"}
                </p>
              ) : null}
            </aside>
          ) : null}
        </>
      ) : null}
    </section>
  );
}

export default GraphLensReactFlow;
