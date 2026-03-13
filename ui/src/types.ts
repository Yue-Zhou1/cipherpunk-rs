export type StepId = 1 | 2 | 3 | 4 | 5 | 6;

export type SourceMode = "git" | "local" | "archive";

export type AppMode =
  | { kind: "wizard" }
  | { kind: "workstation"; sessionId: string };

export type CrateStatus = "in_scope" | "excluded" | "ambiguous";

export type ResolvedCrateStatus = "in_scope" | "excluded";

export type FindingSeverity = "Critical" | "High" | "Medium" | "Low" | "Observation";

export type FindingCategory = "Crypto Misuse" | "Distributed" | "Economic";

export type OutputType =
  | "executive_pdf"
  | "technical_pdf"
  | "evidence_pack_zip"
  | "findings_sarif"
  | "findings_json"
  | "regression_tests_zip";

export type StepDefinition = {
  id: StepId;
  label: string;
  title: string;
};

export type CrateRecord = {
  name: string;
  status: CrateStatus;
  reason?: string;
};

export type CdgNode = {
  id: string;
  risk?: boolean;
};

export type CdgEdge = {
  from: string;
  to: string;
};

export type CdgGraph = {
  nodes: CdgNode[];
  edges: CdgEdge[];
};

export type TraceEvent = {
  tick: number;
  node: string;
  event: string;
  violation?: boolean;
};

export type DistributedTrace = {
  seed: string;
  durationTicks: number;
  events: TraceEvent[];
  violationSummary: string;
};

export type FindingRecord = {
  id: string;
  severity: FindingSeverity;
  category: FindingCategory;
  title: string;
  framework: string;
  ruleId: string;
  verificationStatus: "Verified" | "Unverified";
  llmGenerated: boolean;
  description: string;
  affected: string;
  recommendation: string;
  codeSnippet: string;
  reproduceScript: string;
  evidenceFiles: Array<{ name: string; content: string }>;
  cdg?: CdgGraph;
  trace?: DistributedTrace;
};
