export type ParameterInfo = {
  name: string;
  typeAnnotation?: string;
  position: number;
};

export type FunctionSignature = {
  parameters: ParameterInfo[];
  returnType?: string;
};

export type ExplorerNodeKind =
  | "crate"
  | "module"
  | "file"
  | "function"
  | "trait_impl_method"
  | "macro_call";

export type ExplorerNode = {
  id: string;
  label: string;
  kind: ExplorerNodeKind;
  filePath?: string;
  line?: number;
  signature?: FunctionSignature;
  childCount?: number;
};

export type ExplorerEdgeRelation =
  | "calls"
  | "contains"
  | "invokes_macro"
  | "parameter_flow"
  | "return_flow"
  | "cfg";

export type ExplorerEdge = {
  from: string;
  to: string;
  relation: ExplorerEdgeRelation;
  parameterName?: string;
  parameterPosition?: number;
  valuePreview?: string;
};

export type ExplorerGraph = {
  nodes: ExplorerNode[];
  edges: ExplorerEdge[];
};

export type GranularityLevel = "auto" | "files" | "modules" | "crates";

export type ExplorerStateKind = "overview" | "focus" | "highlight";

export type TraceDirection = "upstream" | "downstream";

export type NeighborhoodResult = {
  highlightedIds: Set<string>;
  direction: TraceDirection;
};

export type ExplorerContextValue = {
  sessionId: string;
  graph: ExplorerGraph;
  stateKind: ExplorerStateKind;
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

  focusedNodeId: string | null;
  upstreamIds: Set<string>;
  downstreamIds: Set<string>;
  totalUpstreamCount: number;
  totalDownstreamCount: number;
  focusNode: (nodeId: string) => void;
  clearFocus: () => void;

  neighborhoodResult: NeighborhoodResult | null;
  showCallers: () => void;
  showCallees: () => void;
  clearHighlight: () => void;

  depth: number;
  setDepth: (depth: number) => void;

  granularity: GranularityLevel;
  setGranularity: (level: GranularityLevel) => void;
  resolvedGranularity: "files" | "modules" | "crates";
  thresholds: { small: number; large: number };
  setThresholds: (thresholds: { small: number; large: number }) => void;

  searchQuery: string;
  setSearchQuery: (query: string) => void;
  matchingNodeIds: Set<string> | null;
  searchHint: string | null;

  expandedClusters: Set<string>;
  toggleCluster: (clusterId: string) => void;

  onNavigateToSource?: (filePath: string, line?: number) => void;
};
