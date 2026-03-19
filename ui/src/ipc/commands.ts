import type { CrateStatus, OutputType } from "../types";

export type SourceInputIpc = {
  kind: "git" | "local" | "archive";
  value: string;
  commitOrRef?: string;
};

export type ResolveSourceResponse = {
  commitHash: string;
  branchResolutionBanner?: string;
  warnings: string[];
};

export type ConfirmWorkspaceRequest = {
  confirmed: boolean;
  ambiguousCrates: Record<string, boolean>;
};

export type ConfirmWorkspaceResponse = {
  auditId: string;
};

export type SessionJob = {
  jobId: string;
  kind: string;
  status: string;
};

export type CreateAuditSessionResponse = {
  sessionId: string;
  snapshotId: string;
  initialJobs: SessionJob[];
};

export type AuditSessionSummary = {
  sessionId: string;
  snapshotId: string;
  createdAt: string;
  updatedAt: string;
};

export type OpenAuditSessionResponse = {
  sessionId: string;
  snapshotId: string;
  selectedDomains: string[];
  initialJobs: SessionJob[];
};

export type ProjectTreeNode = {
  name: string;
  path: string;
  kind: "directory" | "file";
  children: ProjectTreeNode[];
};

export type GetProjectTreeResponse = {
  sessionId: string;
  rootName: string;
  nodes: ProjectTreeNode[];
};

export type ReadSourceFileResponse = {
  sessionId: string;
  path: string;
  content: string;
};

export type SessionConsoleLevel = "info" | "warning" | "error";

export type SessionConsoleEntry = {
  timestamp: string;
  source: string;
  level: SessionConsoleLevel;
  message: string;
};

export type TailSessionConsoleResponse = {
  sessionId: string;
  entries: SessionConsoleEntry[];
};

export type GraphLensKind = "file" | "feature" | "dataflow";

export type ProjectGraphNode = {
  id: string;
  label: string;
  kind: string;
  filePath?: string;
};

export type ProjectGraphEdge = {
  from: string;
  to: string;
  relation: string;
  valuePreview?: string;
};

export type ProjectGraphResponse = {
  sessionId: string;
  lens: GraphLensKind;
  redactedValues: boolean;
  nodes: ProjectGraphNode[];
  edges: ProjectGraphEdge[];
};

export type SecurityOverviewResponse = {
  sessionId: string;
  assets: string[];
  trustBoundaries: string[];
  hotspots: string[];
  reviewNotes: string[];
};

export type ChecklistDomainPlan = {
  id: string;
  rationale: string;
};

export type ChecklistPlanResponse = {
  sessionId: string;
  domains: ChecklistDomainPlan[];
};

export type ToolbenchSelection = {
  kind: "symbol" | "file" | "session";
  id: string;
};

export type ToolbenchRecommendation = {
  toolId: string;
  rationale: string;
};

export type ToolbenchSimilarCase = {
  id: string;
  title: string;
  summary: string;
};

export type ToolbenchContextResponse = {
  sessionId: string;
  selection: ToolbenchSelection;
  recommendedTools: ToolbenchRecommendation[];
  domains: ChecklistDomainPlan[];
  overviewNotes: string[];
  similarCases: ToolbenchSimilarCase[];
};

export type ReviewQueueItem = {
  recordId: string;
  kind: "candidate" | "finding" | "review_note";
  title: string;
  summary: string;
  severity?: "critical" | "high" | "medium" | "low" | "observation";
  verificationStatus: "verified" | "unverified";
  labels: string[];
  evidenceRefs: string[];
  irNodeIds?: string[];
};

export type LoadReviewQueueResponse = {
  sessionId: string;
  items: ReviewQueueItem[];
};

export type ReviewDecisionAction = "confirm" | "reject" | "suppress" | "annotate";

export type ApplyReviewDecisionRequest = {
  recordId: string;
  action: ReviewDecisionAction;
  note?: string;
};

export type ApplyReviewDecisionResponse = {
  sessionId: string;
  item: ReviewQueueItem;
};

export type DownloadOutputResponse = {
  dest: string;
};

export type AuditManifestResponse = Record<string, unknown>;

export type WorkspaceCrateSummary = {
  name: string;
  status: CrateStatus;
  reason?: string;
};

export type BuildVariantSummary = {
  variant: string;
  features: string;
  estTime: string;
};

export type DetectWorkspaceResponse = {
  crateCount: number;
  crates: WorkspaceCrateSummary[];
  frameworks: string[];
  warnings: string[];
  buildMatrix: BuildVariantSummary[];
};

export type ExecutionNodeStatus = "done" | "running" | "waiting";

export type ExecutionNode = {
  name: string;
  channel: "intake" | "rules" | "z3" | "report";
  status: ExecutionNodeStatus;
};

export type ExecutionLogEntry = {
  timestamp: string;
  channel: ExecutionNode["channel"];
  message: string;
};

export type ExecutionCounts = {
  critical: number;
  high: number;
  medium: number;
  low: number;
  observation: number;
};

export type ExecutionUpdateEvent = {
  auditId: string;
  nodes: ExecutionNode[];
  counts: ExecutionCounts;
  logs: ExecutionLogEntry[];
  latestFinding: string;
};

type InvokeFn = <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
type ListenFn = <T>(
  event: string,
  handler: (event: { event: string; id: number; payload: T }) => void
) => Promise<() => void>;
type SaveDialogFn = (options?: { defaultPath?: string }) => Promise<string | null>;

declare global {
  interface Window {
    __TAURI__?: {
      core?: {
        invoke?: InvokeFn;
      };
      event?: {
        listen?: ListenFn;
      };
      dialog?: {
        save?: SaveDialogFn;
      };
    };
  }
}

function tauriInvoke<T>(
  command: string,
  args: Record<string, unknown>,
  fallback: () => Promise<T>
): Promise<T> {
  const invoke = window.__TAURI__?.core?.invoke;
  if (!invoke) {
    return fallback();
  }

  return invoke<T>(command, args);
}

export function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && typeof window.__TAURI__?.core?.invoke === "function";
}

function isFullSha(value: string | undefined): boolean {
  if (!value) {
    return false;
  }

  return /^[a-fA-F0-9]{40}$/.test(value);
}

export async function resolveSource(input: SourceInputIpc): Promise<ResolveSourceResponse> {
  return tauriInvoke("resolve_source", { input }, async () => {
    const shouldShowBanner = input.kind === "git" && !isFullSha(input.commitOrRef);
    return {
      commitHash: "a1b2c3d4ef5678",
      branchResolutionBanner: shouldShowBanner
        ? "Resolved to SHA a1b2c3 - audit is pinned to this commit"
        : undefined,
      warnings: shouldShowBanner
        ? ["Branch reference resolved to pinned commit SHA a1b2c3."]
        : [],
    };
  });
}

export async function parseConfig(path: string): Promise<{ status: "validated" | "errors" }> {
  return tauriInvoke("parse_config", { path }, async () => ({ status: "validated" }));
}

export async function detectWorkspace(): Promise<DetectWorkspaceResponse> {
  return tauriInvoke("detect_workspace", {}, async () => ({
    crateCount: 3,
    crates: [
      { name: "circuit-core", status: "in_scope" },
      { name: "test-utils", status: "excluded", reason: "dev-only" },
      { name: "bridge-adapter", status: "ambiguous" },
    ],
    frameworks: ["Circom", "Halo2", "Rust Crypto"],
    warnings: ["LLM key missing. Degraded features: assume hints, prose polish"],
    buildMatrix: [
      { variant: "default", features: "default", estTime: "~30 min" },
      { variant: "asm", features: "default + asm", estTime: "~35 min" },
    ],
  }));
}

export async function confirmWorkspace(
  decisions: ConfirmWorkspaceRequest
): Promise<ConfirmWorkspaceResponse> {
  return tauriInvoke("confirm_workspace", { decisions }, async () => ({ auditId: "audit-20260305-a1b2c3d4" }));
}

export async function createAuditSession(): Promise<CreateAuditSessionResponse> {
  return tauriInvoke("create_audit_session", {}, async () => ({
    sessionId: "sess-20260305-a1b2",
    snapshotId: "snap-20260305-a1b2",
    initialJobs: [
      { jobId: "job-1", kind: "build_project_ir", status: "queued" },
      { jobId: "job-2", kind: "generate_ai_overview", status: "queued" },
      { jobId: "job-3", kind: "plan_checklists", status: "queued" },
      { jobId: "job-4", kind: "export_reports", status: "queued" },
    ],
  }));
}

export async function listAuditSessions(): Promise<AuditSessionSummary[]> {
  return tauriInvoke("list_audit_sessions", {}, async () => []);
}

export async function openAuditSession(
  sessionId: string
): Promise<OpenAuditSessionResponse | null> {
  return tauriInvoke("open_audit_session", { session_id: sessionId }, async () => null);
}

const FALLBACK_PROJECT_TREE: ProjectTreeNode[] = [
  {
    name: "crates",
    path: "crates",
    kind: "directory",
    children: [
      {
        name: "core",
        path: "crates/core",
        kind: "directory",
        children: [
          {
            name: "src",
            path: "crates/core/src",
            kind: "directory",
            children: [
              {
                name: "session.rs",
                path: "crates/core/src/session.rs",
                kind: "file",
                children: [],
              },
            ],
          },
        ],
      },
    ],
  },
  {
    name: "README.md",
    path: "README.md",
    kind: "file",
    children: [],
  },
];

const FALLBACK_FILE_CONTENTS: Record<string, string> = {
  "README.md": "# Audit Agent\n\nWorkstation fallback content.\n",
  "crates/core/src/session.rs":
    "pub struct AuditSession {\n    pub session_id: String,\n    pub selected_domains: Vec<String>,\n}\n",
};

const FALLBACK_CONSOLE_ENTRIES: SessionConsoleEntry[] = [
  {
    timestamp: "14:23:01",
    source: "bootstrap",
    level: "info",
    message: "Session created and workstation initialized.",
  },
  {
    timestamp: "14:23:05",
    source: "project_ir",
    level: "info",
    message: "Queued build_project_ir job.",
  },
  {
    timestamp: "14:23:08",
    source: "copilot",
    level: "warning",
    message: "LLM key missing. Running deterministic-only mode.",
  },
];

const FALLBACK_FILE_GRAPH: ProjectGraphResponse = {
  sessionId: "sess-fallback",
  lens: "file",
  redactedValues: true,
  nodes: [
    { id: "f1", label: "crates/core/src/session.rs", kind: "file", filePath: "crates/core/src/session.rs" },
    { id: "f2", label: "crates/apps/tauri-ui/src/ipc.rs", kind: "file", filePath: "crates/apps/tauri-ui/src/ipc.rs" },
    { id: "f3", label: "ui/src/features/workstation/WorkstationShell.tsx", kind: "file", filePath: "ui/src/features/workstation/WorkstationShell.tsx" },
  ],
  edges: [
    { from: "f2", to: "f1", relation: "reads-session" },
    { from: "f3", to: "f2", relation: "ipc-calls" },
  ],
};

const FALLBACK_FEATURE_GRAPH: ProjectGraphResponse = {
  sessionId: "sess-fallback",
  lens: "feature",
  redactedValues: true,
  nodes: [
    { id: "feat1", label: "wizard-session-creation", kind: "feature" },
    { id: "feat2", label: "workstation-shell", kind: "feature" },
    { id: "feat3", label: "graph-lens", kind: "feature" },
  ],
  edges: [
    { from: "feat1", to: "feat2", relation: "handoff" },
    { from: "feat2", to: "feat3", relation: "enables" },
  ],
};

const FALLBACK_DATAFLOW_GRAPH_REDACTED: ProjectGraphResponse = {
  sessionId: "sess-fallback",
  lens: "dataflow",
  redactedValues: true,
  nodes: [
    { id: "d1", label: "source-input", kind: "dataflow" },
    { id: "d2", label: "workspace-summary", kind: "dataflow" },
    { id: "d3", label: "session-store", kind: "dataflow" },
  ],
  edges: [
    { from: "d1", to: "d2", relation: "normalize" },
    { from: "d2", to: "d3", relation: "persist" },
  ],
};

const FALLBACK_DATAFLOW_GRAPH_VALUES: ProjectGraphResponse = {
  ...FALLBACK_DATAFLOW_GRAPH_REDACTED,
  redactedValues: false,
  edges: [
    { from: "d1", to: "d2", relation: "normalize", valuePreview: "git:url + pinned-sha" },
    { from: "d2", to: "d3", relation: "persist", valuePreview: "session_id=sess-..." },
  ],
};

const FALLBACK_SECURITY_OVERVIEW: Omit<SecurityOverviewResponse, "sessionId"> = {
  assets: ["Session store", "Project IR graph pipeline", "Audit records and evidence links"],
  trustBoundaries: [
    "Local source ingestion to deterministic analysis jobs",
    "AI copilot suggestions to human verification boundary",
  ],
  hotspots: [
    "Dataflow edges touching persistence and export paths",
    "Workspace scope decisions for ambiguous crates",
  ],
  reviewNotes: [
    "Overview is AI-assisted and must remain unverified until analyst review.",
    "Dataflow values are redacted by default.",
  ],
};

const FALLBACK_CHECKLIST_PLAN: ChecklistDomainPlan[] = [
  { id: "crypto", rationale: "Rust code paths and signing-related modules detected." },
  { id: "zk", rationale: "IR includes zero-knowledge adjacent workspace markers." },
  { id: "p2p-consensus", rationale: "Session orchestration and event flow imply protocol-style checks." },
];

const FALLBACK_SIMILAR_CASES: Array<ToolbenchSimilarCase & { tags: string[] }> = [
  {
    id: "case-crypto-001",
    title: "Missing domain separation in signer path",
    summary: "Prior audit linked weak context binding to cross-domain signature replay risk.",
    tags: ["crypto", "sign", "symbol"],
  },
  {
    id: "case-zk-002",
    title: "Constraint under-specification in witness checks",
    summary: "A prover path accepted malformed witnesses due to missing boolean/range constraints.",
    tags: ["zk", "circom", "proof"],
  },
  {
    id: "case-p2p-003",
    title: "Partition tolerance regression in consensus workflow",
    summary: "Chaos testing exposed state divergence after delayed quorum rejoin.",
    tags: ["p2p-consensus", "consensus", "network"],
  },
];

const FALLBACK_REVIEW_QUEUE_ITEMS: ReviewQueueItem[] = [
  {
    recordId: "cand-crypto-001",
    kind: "candidate",
    title: "Potential signer replay path",
    summary: "Heuristic hotspot suggests missing domain separation in signer context binding.",
    severity: "high",
    verificationStatus: "unverified",
    labels: ["generated", "crypto"],
    evidenceRefs: ["evidence://pending"],
    irNodeIds: ["f2", "f1"],
  },
];

const reviewQueueStateBySession = new Map<string, ReviewQueueItem[]>();

function cloneReviewQueueItems(items: ReviewQueueItem[]): ReviewQueueItem[] {
  return items.map((item) => ({
    ...item,
    labels: [...item.labels],
    evidenceRefs: [...item.evidenceRefs],
    irNodeIds: [...(item.irNodeIds ?? [])],
  }));
}

function ensureFallbackReviewQueue(sessionId: string): ReviewQueueItem[] {
  const existing = reviewQueueStateBySession.get(sessionId);
  if (existing) {
    return existing;
  }

  const seeded = cloneReviewQueueItems(FALLBACK_REVIEW_QUEUE_ITEMS);
  reviewQueueStateBySession.set(sessionId, seeded);
  return seeded;
}

function applyFallbackReviewDecision(
  item: ReviewQueueItem,
  request: ApplyReviewDecisionRequest
): ReviewQueueItem {
  const next: ReviewQueueItem = {
    ...item,
    labels: [...item.labels],
    evidenceRefs: [...item.evidenceRefs],
    irNodeIds: [...(item.irNodeIds ?? [])],
  };

  if (request.note && request.note.trim().length > 0) {
    next.summary = `${next.summary} Note: ${request.note.trim()}`;
  }

  if (request.action === "confirm") {
    next.kind = "finding";
    next.verificationStatus = "verified";
    next.severity ??= "medium";
    if (!next.labels.includes("confirmed")) {
      next.labels.push("confirmed");
    }
  } else if (request.action === "reject") {
    next.kind = "candidate";
    next.verificationStatus = "unverified";
    if (!next.labels.includes("false-positive")) {
      next.labels.push("false-positive");
    }
  } else if (request.action === "suppress") {
    if (!next.labels.includes("suppressed")) {
      next.labels.push("suppressed");
    }
  } else if (request.action === "annotate") {
    if (!next.labels.includes("annotated")) {
      next.labels.push("annotated");
    }
  }

  return next;
}

export async function getProjectTree(sessionId: string): Promise<GetProjectTreeResponse> {
  return tauriInvoke("get_project_tree", { session_id: sessionId }, async () => ({
    sessionId,
    rootName: "project",
    nodes: FALLBACK_PROJECT_TREE,
  }));
}

export async function readSourceFile(
  sessionId: string,
  path: string
): Promise<ReadSourceFileResponse> {
  return tauriInvoke("read_source_file", { session_id: sessionId, path }, async () => ({
    sessionId,
    path,
    content:
      FALLBACK_FILE_CONTENTS[path] ??
      `// File not found in fallback dataset.\n// path: ${path}\n`,
  }));
}

export async function tailSessionConsole(
  sessionId: string,
  limit = 80
): Promise<TailSessionConsoleResponse> {
  return tauriInvoke("tail_session_console", { session_id: sessionId, limit }, async () => ({
    sessionId,
    entries: FALLBACK_CONSOLE_ENTRIES.slice(-Math.max(1, limit)),
  }));
}

export async function loadFileGraph(sessionId: string): Promise<ProjectGraphResponse> {
  return tauriInvoke("load_file_graph", { session_id: sessionId }, async () => ({
    ...FALLBACK_FILE_GRAPH,
    sessionId,
  }));
}

export async function loadFeatureGraph(sessionId: string): Promise<ProjectGraphResponse> {
  return tauriInvoke("load_feature_graph", { session_id: sessionId }, async () => ({
    ...FALLBACK_FEATURE_GRAPH,
    sessionId,
  }));
}

export async function loadDataflowGraph(
  sessionId: string,
  includeValues = false
): Promise<ProjectGraphResponse> {
  return tauriInvoke(
    "load_dataflow_graph",
    { session_id: sessionId, include_values: includeValues },
    async () => ({
      ...(includeValues ? FALLBACK_DATAFLOW_GRAPH_VALUES : FALLBACK_DATAFLOW_GRAPH_REDACTED),
      sessionId,
    })
  );
}

export async function loadSecurityOverview(
  sessionId: string
): Promise<SecurityOverviewResponse> {
  return tauriInvoke("load_security_overview", { session_id: sessionId }, async () => ({
    sessionId,
    ...FALLBACK_SECURITY_OVERVIEW,
  }));
}

export async function loadChecklistPlan(
  sessionId: string
): Promise<ChecklistPlanResponse> {
  return tauriInvoke("load_checklist_plan", { session_id: sessionId }, async () => ({
    sessionId,
    domains: FALLBACK_CHECKLIST_PLAN,
  }));
}

function deriveToolRecommendations(
  selection: ToolbenchSelection,
  domains: ChecklistDomainPlan[],
  overviewNotes: string[]
): ToolbenchRecommendation[] {
  const recommendations = new Map<string, Set<string>>();
  const add = (toolId: string, rationale: string): void => {
    const reasons = recommendations.get(toolId) ?? new Set<string>();
    reasons.add(rationale);
    recommendations.set(toolId, reasons);
  };

  for (const domain of domains) {
    if (domain.id === "crypto") {
      add("Kani", `Recommended by ${domain.id} checklist.`);
      add("Z3", `Constraint checks aligned with ${domain.id} rationale.`);
      add("Cargo Fuzz", `Input mutation coverage requested by ${domain.id} scope.`);
    } else if (domain.id === "zk") {
      add("Circom Z3", `ZK checklist selected: ${domain.rationale}`);
      add("Z3", `SMT proving flow supports ${domain.id} invariants.`);
    } else if (domain.id === "p2p-consensus") {
      add("MadSim", `Scenario simulation suggested by ${domain.id} checklist.`);
      add("Chaos", `Fault-injection testing suggested by ${domain.id} checklist.`);
    }
  }

  if (selection.kind === "symbol") {
    const id = selection.id.toLowerCase();
    if (id.includes("verify") || id.includes("prove") || id.includes("sig")) {
      add("Kani", "Symbol-level verification target detected.");
      add("Z3", "Symbol-level constraint reasoning target detected.");
    }
  }

  if (
    overviewNotes.some((note) => note.toLowerCase().includes("redacted")) &&
    recommendations.has("Z3")
  ) {
    add("Z3", "Overview flagged redaction-sensitive dataflow.");
  }

  if (recommendations.size === 0) {
    add("Kani", "Fallback deterministic baseline for unresolved selection.");
  }

  return Array.from(recommendations.entries()).map(([toolId, rationaleSet]) => ({
    toolId,
    rationale: Array.from(rationaleSet).join(" "),
  }));
}

function deriveSimilarCases(
  selection: ToolbenchSelection,
  domains: ChecklistDomainPlan[]
): ToolbenchSimilarCase[] {
  const queryTerms = new Set<string>([
    selection.kind.toLowerCase(),
    selection.id.toLowerCase(),
    ...domains.map((domain) => domain.id.toLowerCase()),
  ]);

  const matched = FALLBACK_SIMILAR_CASES.filter((item) =>
    item.tags.some((tag) => queryTerms.has(tag.toLowerCase()))
  );

  return (matched.length > 0 ? matched : FALLBACK_SIMILAR_CASES)
    .slice(0, 3)
    .map(({ id, title, summary }) => ({ id, title, summary }));
}

export async function loadToolbenchContext(
  sessionId: string,
  selection: ToolbenchSelection
): Promise<ToolbenchContextResponse> {
  return tauriInvoke(
    "load_toolbench_context",
    { session_id: sessionId, selection },
    async () => {
      try {
        const [checklistPlan, overview] = await Promise.all([
          loadChecklistPlan(sessionId),
          loadSecurityOverview(sessionId),
        ]);
        return {
          sessionId,
          selection,
          recommendedTools: deriveToolRecommendations(
            selection,
            checklistPlan.domains,
            overview.reviewNotes
          ),
          domains: checklistPlan.domains,
          overviewNotes: overview.reviewNotes,
          similarCases: deriveSimilarCases(selection, checklistPlan.domains),
        };
      } catch {
        return {
          sessionId,
          selection,
          recommendedTools: deriveToolRecommendations(
            selection,
            FALLBACK_CHECKLIST_PLAN,
            FALLBACK_SECURITY_OVERVIEW.reviewNotes
          ),
          domains: FALLBACK_CHECKLIST_PLAN,
          overviewNotes: FALLBACK_SECURITY_OVERVIEW.reviewNotes,
          similarCases: deriveSimilarCases(selection, FALLBACK_CHECKLIST_PLAN),
        };
      }
    }
  );
}

export async function loadReviewQueue(
  sessionId: string
): Promise<LoadReviewQueueResponse> {
  return tauriInvoke("load_review_queue", { session_id: sessionId }, async () => ({
    sessionId,
    items: cloneReviewQueueItems(ensureFallbackReviewQueue(sessionId)),
  }));
}

export async function applyReviewDecision(
  sessionId: string,
  request: ApplyReviewDecisionRequest
): Promise<ApplyReviewDecisionResponse> {
  return tauriInvoke(
    "apply_review_decision",
    { session_id: sessionId, request },
    async () => {
      const queue = ensureFallbackReviewQueue(sessionId);
      const index = queue.findIndex((item) => item.recordId === request.recordId);
      if (index < 0) {
        throw new Error("unknown review record");
      }
      const updated = applyFallbackReviewDecision(queue[index], request);
      queue[index] = updated;
      reviewQueueStateBySession.set(sessionId, queue);
      return { sessionId, item: { ...updated } };
    }
  );
}

export async function exportAuditYaml(path: string): Promise<void> {
  await tauriInvoke("export_audit_yaml", { path }, async () => undefined);
}

export async function downloadOutput(
  auditId: string,
  outputType: OutputType,
  dest: string
): Promise<DownloadOutputResponse> {
  return tauriInvoke(
    "download_output",
    { auditId, outputType, dest },
    async () => ({ dest })
  );
}

export async function getAuditManifest(): Promise<AuditManifestResponse> {
  return tauriInvoke(
    "get_audit_manifest",
    {},
    async () => ({
      auditId: "audit-20260305-a1b2c3d4",
      riskScore: 65,
      findingCounts: {
        critical: 0,
        high: 3,
        medium: 4,
        low: 3,
        observation: 1,
      },
    })
  );
}

const FALLBACK_EXECUTION_SCRIPT: ExecutionUpdateEvent[] = [
  {
    auditId: "audit-20260305-a1b2c3d4",
    nodes: [
      { name: "Intake", channel: "intake", status: "done" },
      { name: "Rule Eval", channel: "rules", status: "running" },
      { name: "Z3 Check", channel: "z3", status: "waiting" },
      { name: "Report", channel: "report", status: "waiting" },
    ],
    counts: { critical: 0, high: 1, medium: 2, low: 0, observation: 1 },
    logs: [
      { timestamp: "14:23:01", channel: "intake", message: "Cloning repo" },
      { timestamp: "14:23:05", channel: "intake", message: "12 crates detected" },
      { timestamp: "14:23:08", channel: "rules", message: "Evaluating crypto misuse rules" },
    ],
    latestFinding: "F-ZK-0042 High - canonicality check missing",
  },
  {
    auditId: "audit-20260305-a1b2c3d4",
    nodes: [
      { name: "Intake", channel: "intake", status: "done" },
      { name: "Rule Eval", channel: "rules", status: "done" },
      { name: "Z3 Check", channel: "z3", status: "running" },
      { name: "Report", channel: "report", status: "waiting" },
    ],
    counts: { critical: 0, high: 2, medium: 2, low: 1, observation: 1 },
    logs: [
      { timestamp: "14:23:12", channel: "z3", message: "Running query #17" },
      { timestamp: "14:23:19", channel: "report", message: "Preparing evidence manifest" },
    ],
    latestFinding: "F-CR-111 Critical - signature forgery path",
  },
];

export function subscribeExecutionUpdates(
  auditId: string,
  onUpdate: (update: ExecutionUpdateEvent) => void
): () => void {
  const tauriListen = window.__TAURI__?.event?.listen;
  if (tauriListen) {
    let disposed = false;
    let unlisten: (() => void) | null = null;

    tauriListen<ExecutionUpdateEvent>("audit_execution_update", (event) => {
      if (event.payload.auditId === auditId) {
        onUpdate(event.payload);
      }
    })
      .then((stop) => {
        if (disposed) {
          stop();
          return;
        }
        unlisten = stop;
      })
      .catch(() => undefined);

    return () => {
      disposed = true;
      if (unlisten) {
        unlisten();
      }
    };
  }

  let cursor = 0;
  onUpdate(FALLBACK_EXECUTION_SCRIPT[0]);
  const timer = window.setInterval(() => {
    cursor = Math.min(cursor + 1, FALLBACK_EXECUTION_SCRIPT.length - 1);
    const update = FALLBACK_EXECUTION_SCRIPT[cursor];
    onUpdate({ ...update, auditId });
  }, 1400);

  return () => window.clearInterval(timer);
}

export async function chooseSavePath(defaultName: string): Promise<string | null> {
  const dialogSave = window.__TAURI__?.dialog?.save;
  if (dialogSave) {
    const selected = await dialogSave({ defaultPath: defaultName });
    return typeof selected === "string" && selected.trim().length > 0 ? selected : null;
  }

  if (typeof window.prompt === "function") {
    const selected = window.prompt("Save file path", defaultName);
    if (selected && selected.trim().length > 0) {
      return selected.trim();
    }
  }

  return null;
}
