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

function edgeIdentity(edge: Pick<ExplorerEdgeResponse, "from" | "to" | "relation" | "parameterName" | "parameterPosition">): string {
  return [
    edge.from,
    edge.to,
    edge.relation,
    edge.parameterName ?? "",
    edge.parameterPosition ?? "",
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

export function useUnifiedGraph(sessionId: string): {
  graph: ExplorerGraph;
  nodeMap: Map<string, ExplorerNode>;
  isLoading: boolean;
  loadingClusters: Set<string>;
  error: string | null;
  isStale: boolean;
  expandCluster: (clusterId: string) => void;
  reload: () => void;
} {
  const [graph, setGraph] = useState<ExplorerGraph>(EMPTY_GRAPH);
  const [isLoading, setIsLoading] = useState(true);
  const [loadingClusters, setLoadingClusters] = useState<Set<string>>(new Set());
  const [loadedClusters, setLoadedClusters] = useState<Set<string>>(new Set());
  const [error, setError] = useState<string | null>(null);
  const [isStale, setIsStale] = useState(false);
  const generationRef = useRef(0);

  const nodeMap = useMemo(() => {
    const map = new Map<string, ExplorerNode>();
    for (const node of graph.nodes) {
      map.set(node.id, node);
    }
    return map;
  }, [graph.nodes]);

  useEffect(() => {
    const generation = ++generationRef.current;
    setIsLoading(true);
    setError(null);
    setGraph(EMPTY_GRAPH);
    setLoadingClusters(new Set());
    setLoadedClusters(new Set());
    setIsStale(false);

    void loadExplorerGraph(sessionId, "overview").then(
      (response) => {
        if (generation !== generationRef.current) {
          return;
        }
        setGraph({
          nodes: response.nodes.map(toExplorerNode),
          edges: response.edges.map(toExplorerEdge),
        });
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
  }, [sessionId]);

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

      void loadExplorerGraph(sessionId, undefined, clusterId).then(
        (response) => {
          if (generation !== generationRef.current) {
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
        () => {
          if (generation !== generationRef.current) {
            return;
          }
          setLoadingClusters((previous) => {
            const next = new Set(previous);
            next.delete(clusterId);
            return next;
          });
        }
      );
    },
    [sessionId, loadedClusters, loadingClusters]
  );

  const reload = useCallback(() => {
    generationRef.current += 1;
    setIsStale(false);
    setIsLoading(true);
    setError(null);
    setGraph(EMPTY_GRAPH);
    setLoadingClusters(new Set());
    setLoadedClusters(new Set());

    const generation = generationRef.current;
    void loadExplorerGraph(sessionId, "overview").then(
      (response) => {
        if (generation !== generationRef.current) {
          return;
        }
        setGraph({
          nodes: response.nodes.map(toExplorerNode),
          edges: response.edges.map(toExplorerEdge),
        });
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
  }, [sessionId]);

  return {
    graph,
    nodeMap,
    isLoading,
    loadingClusters,
    error,
    isStale,
    expandCluster,
    reload,
  };
}
