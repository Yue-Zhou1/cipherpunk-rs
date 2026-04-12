import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import {
  type ExplorerEdgeResponse,
  type ExplorerGraphResponse,
  type ExplorerNodeResponse,
  loadExplorerGraph,
} from "../../../../ipc/commands";
import { getTransport } from "../../../../ipc/transport";
import type { ExplorerEdge, ExplorerGraph, ExplorerNode } from "../types";

const EMPTY_GRAPH: ExplorerGraph = { nodes: [], edges: [] };

function toExplorerNode(node: ExplorerNodeResponse): ExplorerNode {
  return {
    id: node.id,
    label: node.label,
    kind: node.kind as ExplorerNode["kind"],
    filePath: node.filePath,
    line: node.line,
    signature: node.signature
      ? {
          parameters: node.signature.parameters.map((parameter) => ({
            name: parameter.name,
            typeAnnotation: parameter.typeAnnotation,
            position: parameter.position,
          })),
          returnType: node.signature.returnType,
        }
      : undefined,
    childCount: node.childCount,
  };
}

function toExplorerEdge(edge: ExplorerEdgeResponse): ExplorerEdge {
  return {
    from: edge.from,
    to: edge.to,
    relation: edge.relation as ExplorerEdge["relation"],
    parameterName: edge.parameterName,
    parameterPosition: edge.parameterPosition,
    valuePreview: edge.valuePreview,
  };
}

function edgeIdentity(
  edge: Pick<
    ExplorerEdgeResponse,
    "from" | "to" | "relation" | "parameterName" | "parameterPosition" | "valuePreview"
  >
): string {
  return [
    edge.from,
    edge.to,
    edge.relation,
    edge.parameterName ?? "",
    edge.parameterPosition ?? "",
    edge.valuePreview ?? "",
  ].join("->");
}

function mergeClusterData(current: ExplorerGraph, expansion: ExplorerGraphResponse): ExplorerGraph {
  const existingNodeIds = new Set(current.nodes.map((node) => node.id));
  const newNodes = expansion.nodes
    .filter((node) => !existingNodeIds.has(node.id))
    .map(toExplorerNode);

  const existingEdgeKeys = new Set(current.edges.map((edge) => edgeIdentity(edge)));
  const newEdges = expansion.edges
    .filter((edge) => !existingEdgeKeys.has(edgeIdentity(edge)))
    .map(toExplorerEdge);

  return {
    nodes: [...current.nodes, ...newNodes],
    edges: [...current.edges, ...newEdges],
  };
}

export function useUnifiedGraph(sessionId: string, requestFull: boolean): {
  graph: ExplorerGraph;
  nodeMap: Map<string, ExplorerNode>;
  isLoading: boolean;
  loadingClusters: Set<string>;
  loadedClusters: Set<string>;
  clusterErrors: Map<string, string>;
  error: string | null;
  isStale: boolean;
  expandCluster: (clusterId: string) => void;
  reload: () => void;
  graphDepth: "overview" | "full";
  hasLoadedOverview: boolean;
} {
  const [graph, setGraph] = useState<ExplorerGraph>(EMPTY_GRAPH);
  const [isLoading, setIsLoading] = useState(true);
  const [loadingClusters, setLoadingClusters] = useState<Set<string>>(new Set());
  const [loadedClusters, setLoadedClusters] = useState<Set<string>>(new Set());
  const [clusterErrors, setClusterErrors] = useState<Map<string, string>>(new Map());
  const [error, setError] = useState<string | null>(null);
  const [isStale, setIsStale] = useState(false);
  const [graphDepth, setGraphDepth] = useState<"overview" | "full">("overview");
  const [hasLoadedOverview, setHasLoadedOverview] = useState(false);
  const previousSessionIdRef = useRef<string | null>(null);
  const generationRef = useRef(0);

  const nodeMap = useMemo(() => {
    const map = new Map<string, ExplorerNode>();
    for (const node of graph.nodes) {
      map.set(node.id, node);
    }
    return map;
  }, [graph.nodes]);

  const loadBaseGraph = useCallback(
    (depth: "overview" | "full", resetGraph: boolean) => {
      const generation = ++generationRef.current;
      setIsLoading(true);
      setError(null);
      setLoadingClusters(new Set());
      setLoadedClusters(new Set());
      setClusterErrors(new Map());
      setIsStale(false);
      if (resetGraph) {
        setGraph(EMPTY_GRAPH);
      }

      void loadExplorerGraph(sessionId, depth).then(
        (response) => {
          if (generation !== generationRef.current) {
            return;
          }
          if (!response || !Array.isArray(response.nodes) || !Array.isArray(response.edges)) {
            setError("Failed to load graph");
            setIsLoading(false);
            return;
          }
          setGraph({
            nodes: response.nodes.map(toExplorerNode),
            edges: response.edges.map(toExplorerEdge),
          });
          setGraphDepth(depth);
          if (depth === "overview") {
            setHasLoadedOverview(true);
          }
          setIsLoading(false);
        },
        (loadError) => {
          if (generation !== generationRef.current) {
            return;
          }
          setError(loadError instanceof Error ? loadError.message : "Failed to load graph");
          setIsLoading(false);
        }
      );
    },
    [sessionId]
  );

  useEffect(() => {
    const nextDepth: "overview" | "full" = requestFull ? "full" : "overview";
    const sessionChanged = previousSessionIdRef.current !== sessionId;
    previousSessionIdRef.current = sessionId;

    if (sessionChanged) {
      setHasLoadedOverview(false);
    }

    loadBaseGraph(nextDepth, sessionChanged);
  }, [sessionId, requestFull, loadBaseGraph]);

  useEffect(() => {
    const unsubscribe = getTransport().subscribe<{ event?: string }>(
      "explorer_graph_stale",
      sessionId,
      (payload) => {
        if (payload.event === "explorer_graph_stale") {
          setIsStale(true);
        }
      }
    );

    return unsubscribe;
  }, [sessionId]);

  const expandCluster = useCallback(
    (clusterId: string) => {
      if (loadedClusters.has(clusterId) || loadingClusters.has(clusterId)) {
        return;
      }

      const generation = generationRef.current;
      setLoadingClusters((previous) => {
        const next = new Set(previous);
        next.add(clusterId);
        return next;
      });
      setClusterErrors((previous) => {
        const next = new Map(previous);
        next.delete(clusterId);
        return next;
      });

      void loadExplorerGraph(sessionId, undefined, clusterId).then(
        (response) => {
          if (generation !== generationRef.current) {
            return;
          }
          if (!response || !Array.isArray(response.nodes) || !Array.isArray(response.edges)) {
            setLoadingClusters((previous) => {
              const next = new Set(previous);
              next.delete(clusterId);
              return next;
            });
            setClusterErrors((previous) => {
              const next = new Map(previous);
              next.set(clusterId, "Failed to load");
              return next;
            });
            return;
          }
          setGraph((previous) => mergeClusterData(previous, response));
          setLoadedClusters((previous) => {
            const next = new Set(previous);
            next.add(clusterId);
            return next;
          });
          setLoadingClusters((previous) => {
            const next = new Set(previous);
            next.delete(clusterId);
            return next;
          });
        },
        (expandError) => {
          if (generation !== generationRef.current) {
            return;
          }
          setLoadingClusters((previous) => {
            const next = new Set(previous);
            next.delete(clusterId);
            return next;
          });
          setClusterErrors((previous) => {
            const next = new Map(previous);
            next.set(
              clusterId,
              expandError instanceof Error ? expandError.message : "Failed to load"
            );
            return next;
          });
        }
      );
    },
    [sessionId, loadedClusters, loadingClusters]
  );

  const reload = useCallback(() => {
    loadBaseGraph(requestFull ? "full" : "overview", false);
  }, [loadBaseGraph, requestFull]);

  return {
    graph,
    nodeMap,
    isLoading,
    loadingClusters,
    loadedClusters,
    clusterErrors,
    error,
    isStale,
    expandCluster,
    reload,
    graphDepth,
    hasLoadedOverview,
  };
}
