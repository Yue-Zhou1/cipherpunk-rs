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
