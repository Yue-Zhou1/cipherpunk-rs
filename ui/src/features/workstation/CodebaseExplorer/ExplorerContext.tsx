import {
  createContext,
  useCallback,
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
  const { graph, nodeMap, isLoading, loadingClusters, error, isStale, expandCluster, reload } =
    useUnifiedGraph(sessionId);
  const { depth, setDepth } = useDepthControl();

  const fileCount = useMemo(
    () => graph.nodes.filter((node) => node.kind === "file").length,
    [graph.nodes]
  );
  const adaptive = useAdaptiveThresholds(fileCount);
  const focus = useFocusContext(graph, depth);
  const trace = useTrace(graph, focus.focusedNodeId);

  const [searchQuery, setSearchQuery] = useState("");
  const [expandedClusters, setExpandedClusters] = useState<Set<string>>(new Set());
  const [deadEndMessage, setDeadEndMessage] = useState<string | null>(null);

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

  const hasPotentialParameterFlow = useCallback(
    (parameterName: string): boolean => {
      if (!focus.focusedNodeId) {
        return false;
      }
      return graph.edges.some(
        (edge) =>
          edge.to === focus.focusedNodeId &&
          (edge.relation === "calls" ||
            (edge.relation === "parameter_flow" && edge.parameterName === parameterName))
      );
    },
    [focus.focusedNodeId, graph.edges]
  );

  const hasPotentialReturnFlow = useCallback((): boolean => {
    if (!focus.focusedNodeId) {
      return false;
    }
    return graph.edges.some(
      (edge) =>
        edge.from === focus.focusedNodeId &&
        (edge.relation === "calls" || edge.relation === "return_flow")
    );
  }, [focus.focusedNodeId, graph.edges]);

  const stateKind: ExplorerStateKind = trace.traceResult ? "trace" : focus.stateKind;

  const value: ExplorerContextValue = {
    graph,
    stateKind,
    nodeMap,
    isLoading,
    loadingClusters,
    error,
    isStale,
    expandCluster,
    reload,
    focusedNodeId: focus.focusedNodeId,
    upstreamIds: focus.upstreamIds,
    downstreamIds: focus.downstreamIds,
    focusNode: (nodeId) => {
      setDeadEndMessage(null);
      trace.clearTrace();
      focus.focusNode(nodeId);
    },
    clearFocus: () => {
      setDeadEndMessage(null);
      trace.clearTrace();
      focus.clearFocus();
    },
    traceResult: trace.traceResult,
    traceParameter: (parameterName) => {
      setDeadEndMessage(null);
      if (!hasPotentialParameterFlow(parameterName)) {
        trace.clearTrace();
        setDeadEndMessage(
          `No upstream flow found for "${parameterName}" - value may be constructed locally`
        );
        return;
      }
      trace.traceParameter(parameterName);
    },
    traceReturn: () => {
      setDeadEndMessage(null);
      if (!hasPotentialReturnFlow()) {
        trace.clearTrace();
        setDeadEndMessage("No downstream flow found - return value may not be consumed");
        return;
      }
      trace.traceReturn();
    },
    clearTrace: () => {
      setDeadEndMessage(null);
      trace.clearTrace();
    },
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
    expandedClusters,
    toggleCluster,
    deadEndMessage,
    onNavigateToSource,
  };

  return <ExplorerCtx.Provider value={value}>{children}</ExplorerCtx.Provider>;
}
