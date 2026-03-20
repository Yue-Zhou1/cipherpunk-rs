import type { CrateStatus, OutputType } from "../types";
import { getTransport, isTauriRuntime as hasTauriBridge } from "./transport";

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

type CommandFixturesModule = typeof import("./commands.fixtures");

async function loadCommandFixtures(): Promise<CommandFixturesModule> {
  return import("./commands.fixtures");
}

function tauriInvoke<T>(
  command: string,
  args: Record<string, unknown>,
  fallback?: () => Promise<T>
): Promise<T> {
  const transport = getTransport();
  if (transport.kind === "tauri" && !hasTauriBridge()) {
    if (fallback && import.meta.env.MODE === "test") {
      return fallback();
    }
    return Promise.reject(new Error("Tauri invoke bridge is unavailable"));
  }
  return transport.invoke<T>(command, args);
}

export function isTauriRuntime(): boolean {
  return hasTauriBridge();
}

export async function resolveSource(input: SourceInputIpc): Promise<ResolveSourceResponse> {
  return tauriInvoke("resolve_source", { input }, async () =>
    (await loadCommandFixtures()).resolveSourceFallback(input)
  );
}

export async function parseConfig(path: string): Promise<{ status: "validated" | "errors" }> {
  return tauriInvoke("parse_config", { path }, async () =>
    (await loadCommandFixtures()).parseConfigFallback()
  );
}

export async function detectWorkspace(): Promise<DetectWorkspaceResponse> {
  return tauriInvoke("detect_workspace", {}, async () =>
    (await loadCommandFixtures()).detectWorkspaceFallback()
  );
}

export async function confirmWorkspace(
  decisions: ConfirmWorkspaceRequest
): Promise<ConfirmWorkspaceResponse> {
  return tauriInvoke("confirm_workspace", { decisions }, async () =>
    (await loadCommandFixtures()).confirmWorkspaceFallback()
  );
}

export async function createAuditSession(): Promise<CreateAuditSessionResponse> {
  return tauriInvoke("create_audit_session", {}, async () =>
    (await loadCommandFixtures()).createAuditSessionFallback()
  );
}

export async function listAuditSessions(): Promise<AuditSessionSummary[]> {
  return tauriInvoke("list_audit_sessions", {}, async () =>
    (await loadCommandFixtures()).listAuditSessionsFallback()
  );
}

export async function openAuditSession(
  sessionId: string
): Promise<OpenAuditSessionResponse | null> {
  return tauriInvoke("open_audit_session", { session_id: sessionId }, async () =>
    (await loadCommandFixtures()).openAuditSessionFallback()
  );
}

export async function getProjectTree(sessionId: string): Promise<GetProjectTreeResponse> {
  return tauriInvoke("get_project_tree", { session_id: sessionId }, async () =>
    (await loadCommandFixtures()).getProjectTreeFallback(sessionId)
  );
}

export async function readSourceFile(
  sessionId: string,
  path: string
): Promise<ReadSourceFileResponse> {
  return tauriInvoke("read_source_file", { session_id: sessionId, path }, async () =>
    (await loadCommandFixtures()).readSourceFileFallback(sessionId, path)
  );
}

export async function tailSessionConsole(
  sessionId: string,
  limit = 80
): Promise<TailSessionConsoleResponse> {
  return tauriInvoke("tail_session_console", { session_id: sessionId, limit }, async () =>
    (await loadCommandFixtures()).tailSessionConsoleFallback(sessionId, limit)
  );
}

export async function loadFileGraph(sessionId: string): Promise<ProjectGraphResponse> {
  return tauriInvoke("load_file_graph", { session_id: sessionId }, async () =>
    (await loadCommandFixtures()).loadFileGraphFallback(sessionId)
  );
}

export async function loadFeatureGraph(sessionId: string): Promise<ProjectGraphResponse> {
  return tauriInvoke("load_feature_graph", { session_id: sessionId }, async () =>
    (await loadCommandFixtures()).loadFeatureGraphFallback(sessionId)
  );
}

export async function loadDataflowGraph(
  sessionId: string,
  includeValues = false
): Promise<ProjectGraphResponse> {
  return tauriInvoke(
    "load_dataflow_graph",
    { session_id: sessionId, include_values: includeValues },
    async () => (await loadCommandFixtures()).loadDataflowGraphFallback(sessionId, includeValues)
  );
}

export async function loadSecurityOverview(
  sessionId: string
): Promise<SecurityOverviewResponse> {
  return tauriInvoke("load_security_overview", { session_id: sessionId }, async () =>
    (await loadCommandFixtures()).loadSecurityOverviewFallback(sessionId)
  );
}

export async function loadChecklistPlan(
  sessionId: string
): Promise<ChecklistPlanResponse> {
  return tauriInvoke("load_checklist_plan", { session_id: sessionId }, async () =>
    (await loadCommandFixtures()).loadChecklistPlanFallback(sessionId)
  );
}

export async function loadToolbenchContext(
  sessionId: string,
  selection: ToolbenchSelection
): Promise<ToolbenchContextResponse> {
  return tauriInvoke(
    "load_toolbench_context",
    { session_id: sessionId, selection },
    async () => (await loadCommandFixtures()).loadToolbenchContextFallback(sessionId, selection)
  );
}

export async function loadReviewQueue(
  sessionId: string
): Promise<LoadReviewQueueResponse> {
  return tauriInvoke("load_review_queue", { session_id: sessionId }, async () =>
    (await loadCommandFixtures()).loadReviewQueueFallback(sessionId)
  );
}

export async function applyReviewDecision(
  sessionId: string,
  request: ApplyReviewDecisionRequest
): Promise<ApplyReviewDecisionResponse> {
  return tauriInvoke(
    "apply_review_decision",
    { session_id: sessionId, request },
    async () => (await loadCommandFixtures()).applyReviewDecisionFallback(sessionId, request)
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
    async () => (await loadCommandFixtures()).downloadOutputFallback(dest)
  );
}

export async function getAuditManifest(): Promise<AuditManifestResponse> {
  return tauriInvoke(
    "get_audit_manifest",
    {},
    async () => (await loadCommandFixtures()).getAuditManifestFallback()
  );
}

export function subscribeExecutionUpdates(
  auditId: string,
  onUpdate: (update: ExecutionUpdateEvent) => void
): () => void {
  const transport = getTransport();
  if (transport.kind === "http") {
    return transport.subscribe<ExecutionUpdateEvent>(
      "audit_execution_update",
      auditId,
      (payload) => {
        if (!payload.auditId || payload.auditId === auditId) {
          onUpdate(payload);
        }
      }
    );
  }

  if (isTauriRuntime()) {
    return transport.subscribe<ExecutionUpdateEvent>(
      "audit_execution_update",
      auditId,
      (payload) => {
        if (payload.auditId === auditId) {
          onUpdate(payload);
        }
      }
    );
  }

  if (import.meta.env.MODE !== "test") {
    throw new Error("Execution updates are unavailable without an active Tauri bridge");
  }

  let disposed = false;
  let timer: number | null = null;

  void loadCommandFixtures().then((fixtures) => {
    if (disposed) {
      return;
    }
    const script = fixtures.executionScriptFallback(auditId);
    if (script.length === 0) {
      return;
    }

    let cursor = 0;
    onUpdate(script[0]);
    timer = window.setInterval(() => {
      cursor = Math.min(cursor + 1, script.length - 1);
      onUpdate(script[cursor]);
    }, 1400);
  });

  return () => {
    disposed = true;
    if (timer !== null) {
      window.clearInterval(timer);
    }
  };
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
