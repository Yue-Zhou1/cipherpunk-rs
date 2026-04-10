import {
  createContext,
  useCallback,
  useEffect,
  useContext,
  useMemo,
  useState,
  type ReactNode,
} from "react";

import { useAdaptiveThresholds } from "./hooks/useAdaptiveThresholds";
import { useDepthControl } from "./hooks/useDepthControl";
import { useFocusContext } from "./hooks/useFocusContext";
import { useTrace } from "./hooks/useTrace";
import { useUnifiedGraph } from "./hooks/useUnifiedGraph";
import type { ExplorerContextValue, ExplorerStateKind } from "./types";

const ExplorerCtx = createContext<ExplorerContextValue | null>(null);

export function useExplorer(): ExplorerContextValue {
  const value = useContext(ExplorerCtx);
  if (!value) {
    throw new Error("useExplorer must be used within ExplorerProvider");
  }
  return value;
}

type ExplorerProviderProps = {
  children: ReactNode;
  sessionId: string;
  onNavigateToSource?: (filePath: string, line?: number) => void;
};

export function ExplorerProvider({
  children,
  sessionId,
  onNavigateToSource,
}: ExplorerProviderProps) {
  const [requestFull, setRequestFull] = useState(false);
  const {
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
  } = useUnifiedGraph(sessionId, requestFull);
  const { depth, setDepth } = useDepthControl();

  useEffect(() => {
    setRequestFull(false);
  }, [sessionId]);

  const fileCount = useMemo(
    () => graph.nodes.filter((node) => node.kind === "file").length,
    [graph.nodes]
  );
  const adaptive = useAdaptiveThresholds(fileCount);
  const focus = useFocusContext(graph, depth);
  const trace = useTrace(graph, focus.focusedNodeId);

  useEffect(() => {
    const shouldRequestFull =
      adaptive.resolvedGranularity === "files" &&
      hasLoadedOverview &&
      graphDepth === "overview";
    if (shouldRequestFull && !requestFull) {
      setRequestFull(true);
    }
  }, [adaptive.resolvedGranularity, graphDepth, hasLoadedOverview, requestFull]);

  const [searchQuery, setSearchQuery] = useState("");
  const [expandedClusters, setExpandedClusters] = useState<Set<string>>(new Set());

  const matchingNodeIds = useMemo(() => {
    const query = searchQuery.trim().toLowerCase();
    if (!query) {
      return null;
    }
    return new Set(
      graph.nodes
        .filter((node) => node.label.toLowerCase().includes(query) || node.id.toLowerCase().includes(query))
        .map((node) => node.id)
    );
  }, [graph.nodes, searchQuery]);

  const toggleCluster = useCallback((clusterId: string) => {
    setExpandedClusters((previous) => {
      const next = new Set(previous);
      if (next.has(clusterId)) {
        next.delete(clusterId);
      } else {
        next.add(clusterId);
      }
      return next;
    });
  }, []);

  const hasExpandableClusters = useMemo(
    () =>
      graph.nodes.some(
        (node) =>
          (node.kind === "crate" || node.kind === "module") && !loadedClusters.has(node.id)
      ),
    [graph.nodes, loadedClusters]
  );
  const searchHint =
    searchQuery.trim() && graphDepth !== "full" && hasExpandableClusters
      ? "Searching loaded graph only. Expand clusters to search their contents."
      : null;

  const stateKind: ExplorerStateKind =
    focus.stateKind === "overview"
      ? "overview"
      : trace.neighborhoodResult
        ? "highlight"
        : "focus";

  const value: ExplorerContextValue = {
    sessionId,
    graph,
    stateKind,
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
    focusedNodeId: focus.focusedNodeId,
    upstreamIds: focus.upstreamIds,
    downstreamIds: focus.downstreamIds,
    totalUpstreamCount: focus.totalUpstreamCount,
    totalDownstreamCount: focus.totalDownstreamCount,
    focusNode: (nodeId) => {
      trace.clearHighlight();
      focus.focusNode(nodeId);
    },
    clearFocus: () => {
      trace.clearHighlight();
      focus.clearFocus();
    },
    neighborhoodResult: trace.neighborhoodResult,
    showCallers: trace.showCallers,
    showCallees: trace.showCallees,
    clearHighlight: trace.clearHighlight,
    depth,
    setDepth,
    granularity: adaptive.granularity,
    setGranularity: adaptive.setGranularity,
    resolvedGranularity: adaptive.resolvedGranularity,
    thresholds: adaptive.thresholds,
    setThresholds: adaptive.setThresholds,
    searchQuery,
    setSearchQuery,
    matchingNodeIds,
    searchHint,
    expandedClusters,
    toggleCluster,
    onNavigateToSource,
  };

  return <ExplorerCtx.Provider value={value}>{children}</ExplorerCtx.Provider>;
}
