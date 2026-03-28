import type {
  ActivitySummary,
  ApplyReviewDecisionRequest,
  ApplyReviewDecisionResponse,
  AuditPlanResponse,
  AuditManifestResponse,
  AuditSessionSummary,
  ChecklistDomainPlan,
  ChecklistPlanResponse,
  ConfirmWorkspaceResponse,
  CreateAuditSessionResponse,
  DetectWorkspaceResponse,
  DownloadOutputResponse,
  ExecutionUpdateEvent,
  GetProjectTreeResponse,
  LoadReviewQueueResponse,
  OpenAuditSessionResponse,
  ReadSourceFileResponse,
  ResolveSourceResponse,
  ReviewQueueItem,
  SecurityOverviewResponse,
  SessionConsoleEntry,
  SourceInputIpc,
  TailSessionConsoleResponse,
  ToolbenchContextResponse,
  ToolbenchSelection,
  ToolbenchSimilarCase,
} from "./commands";

const FALLBACK_PROJECT_TREE: GetProjectTreeResponse["nodes"] = [
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
  "README.md": "# Audit Agent\\n\\nWorkstation fallback content.\\n",
  "crates/core/src/session.rs":
    "pub struct AuditSession {\\n    pub session_id: String,\\n    pub selected_domains: Vec<String>,\\n}\\n",
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

const FALLBACK_ACTIVITY_SUMMARY: Omit<ActivitySummary, "sessionId"> = {
  llmCalls: [
    {
      role: "SearchHints",
      count: 2,
      avgDurationMs: 85,
      totalPromptChars: 510,
      totalResponseChars: 930,
      providersUsed: ["openai"],
      succeeded: 2,
      failed: 0,
    },
  ],
  toolActions: [
    {
      toolFamily: "kani",
      count: 1,
      succeeded: 1,
      failed: 0,
      avgDurationMs: 42,
    },
  ],
  reviewDecisions: [{ action: "confirm", count: 1 }],
  engineOutcomes: [
    { engine: "crypto_zk", status: "completed", findingsCount: 2, durationMs: 133 },
    { engine: "distributed", status: "failed", findingsCount: 0, durationMs: 0 },
  ],
  totalEvents: 5,
  totalDurationMs: 303,
};

const FALLBACK_AUDIT_PLAN: Omit<AuditPlanResponse, "sessionId"> = {
  planId: "plan-fallback-1",
  overview: {
    assets: ["Session store", "IR pipeline", "Workstation API"],
    trustBoundaries: ["Source intake boundary", "Analyst decision boundary"],
    hotspots: ["Persistence writes", "Graph rendering bridge"],
  },
  domains: [
    { id: "crypto", rationale: "workspace includes crypto-related crates and symbols" },
    { id: "zk", rationale: "IR features include zk-adjacent patterns" },
  ],
  recommendedTools: [
    { tool: "Kani", rationale: "deterministic baseline for symbol-level invariants" },
    { tool: "Z3", rationale: "constraint solving for arithmetic/path invariants" },
  ],
  engines: {
    cryptoZk: true,
    distributed: false,
  },
  rationale:
    "Generated from deterministic workspace analysis and checklist/tool recommendation synthesis.",
  createdAt: new Date("2026-03-23T10:00:00Z").toISOString(),
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
  {
    id: "p2p-consensus",
    rationale: "Session orchestration and event flow imply protocol-style checks.",
  },
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

function deriveToolRecommendations(
  selection: ToolbenchSelection,
  domains: ChecklistDomainPlan[],
  overviewNotes: string[]
): ToolbenchContextResponse["recommendedTools"] {
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

export function resolveSourceFallback(input: SourceInputIpc): ResolveSourceResponse {
  const shouldShowBanner = input.kind === "git" && !/^[a-fA-F0-9]{40}$/.test(input.commitOrRef ?? "");
  return {
    commitHash: "a1b2c3d4ef5678",
    branchResolutionBanner: shouldShowBanner
      ? "Resolved to SHA a1b2c3 - audit is pinned to this commit"
      : undefined,
    warnings: shouldShowBanner ? ["Branch reference resolved to pinned commit SHA a1b2c3."] : [],
  };
}

export function parseConfigFallback(): { status: "validated" | "errors" } {
  return { status: "validated" };
}

export function detectWorkspaceFallback(): DetectWorkspaceResponse {
  return {
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
  };
}

export function confirmWorkspaceFallback(): ConfirmWorkspaceResponse {
  return { auditId: "audit-20260305-a1b2c3d4" };
}

export function createAuditSessionFallback(): CreateAuditSessionResponse {
  return {
    sessionId: "sess-20260305-a1b2",
    snapshotId: "snap-20260305-a1b2",
    initialJobs: [
      { jobId: "job-1", kind: "build_project_ir", status: "queued" },
      { jobId: "job-2", kind: "generate_ai_overview", status: "queued" },
      { jobId: "job-3", kind: "plan_checklists", status: "queued" },
      { jobId: "job-4", kind: "export_reports", status: "queued" },
    ],
  };
}

export function listAuditSessionsFallback(): AuditSessionSummary[] {
  return [];
}

export function openAuditSessionFallback(): OpenAuditSessionResponse | null {
  return null;
}

export function getProjectTreeFallback(sessionId: string): GetProjectTreeResponse {
  return { sessionId, rootName: "project", nodes: FALLBACK_PROJECT_TREE };
}

export function readSourceFileFallback(
  sessionId: string,
  path: string
): ReadSourceFileResponse {
  return {
    sessionId,
    path,
    content:
      FALLBACK_FILE_CONTENTS[path] ??
      `// File not found in fallback dataset.\\n// path: ${path}\\n`,
  };
}

export function tailSessionConsoleFallback(
  sessionId: string,
  limit = 80
): TailSessionConsoleResponse {
  return { sessionId, entries: FALLBACK_CONSOLE_ENTRIES.slice(-Math.max(1, limit)) };
}

export function loadActivitySummaryFallback(sessionId: string): ActivitySummary {
  return { sessionId, ...FALLBACK_ACTIVITY_SUMMARY };
}

export function loadAuditPlanFallback(sessionId: string): AuditPlanResponse {
  return { sessionId, ...FALLBACK_AUDIT_PLAN };
}

export function loadSecurityOverviewFallback(sessionId: string): SecurityOverviewResponse {
  return { sessionId, ...FALLBACK_SECURITY_OVERVIEW };
}

export function loadChecklistPlanFallback(sessionId: string): ChecklistPlanResponse {
  return { sessionId, domains: FALLBACK_CHECKLIST_PLAN };
}

export function loadToolbenchContextFallback(
  sessionId: string,
  selection: ToolbenchSelection
): ToolbenchContextResponse {
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

export function loadReviewQueueFallback(sessionId: string): LoadReviewQueueResponse {
  return {
    sessionId,
    items: cloneReviewQueueItems(ensureFallbackReviewQueue(sessionId)),
  };
}

export function applyReviewDecisionFallback(
  sessionId: string,
  request: ApplyReviewDecisionRequest
): ApplyReviewDecisionResponse {
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

export function downloadOutputFallback(dest: string): DownloadOutputResponse {
  return { dest };
}

export function getAuditManifestFallback(): AuditManifestResponse {
  return {
    auditId: "audit-20260305-a1b2c3d4",
    riskScore: 65,
    findingCounts: {
      critical: 0,
      high: 3,
      medium: 4,
      low: 3,
      observation: 1,
    },
  };
}

export function executionScriptFallback(auditId: string): ExecutionUpdateEvent[] {
  return FALLBACK_EXECUTION_SCRIPT.map((item) => ({ ...item, auditId }));
}
