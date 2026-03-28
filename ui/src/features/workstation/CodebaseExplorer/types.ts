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

export type ExplorerStateKind = "overview" | "focus" | "trace";

export type TraceDirection = "upstream" | "downstream";

export type TraceResult = {
  path: string[];
  direction: TraceDirection;
  parameterName?: string;
};

export type ExplorerContextValue = {
  graph: ExplorerGraph;
  stateKind: ExplorerStateKind;
  nodeMap: Map<string, ExplorerNode>;
  isLoading: boolean;
  loadingClusters: Set<string>;
  error: string | null;
  isStale: boolean;
  expandCluster: (clusterId: string) => void;
  reload: () => void;

  focusedNodeId: string | null;
  upstreamIds: Set<string>;
  downstreamIds: Set<string>;
  focusNode: (nodeId: string) => void;
  clearFocus: () => void;

  traceResult: TraceResult | null;
  traceParameter: (parameterName: string) => void;
  traceReturn: () => void;
  clearTrace: () => void;

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

  expandedClusters: Set<string>;
  toggleCluster: (clusterId: string) => void;

  deadEndMessage: string | null;

  onNavigateToSource?: (filePath: string, line?: number) => void;
};
