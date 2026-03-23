type InvokeFn = <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
type ListenFn = <T>(
  event: string,
  handler: (event: { event: string; id: number; payload: T }) => void
) => Promise<() => void>;

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
        save?: (options?: { defaultPath?: string }) => Promise<string | null>;
      };
    };
  }
}

export type TransportKind = "tauri" | "http";

export interface Transport {
  readonly kind: TransportKind;
  invoke<T>(command: string, args: Record<string, unknown>): Promise<T>;
  subscribe<T>(
    event: string,
    sessionId: string,
    handler: (payload: T) => void
  ): () => void;
}

function normalizeBaseUrl(baseUrl: string): string {
  if (!baseUrl) {
    return "";
  }
  return baseUrl.replace(/\/+$/, "");
}

function joinBaseUrlAndPath(baseUrl: string, path: string): string {
  const normalizedBase = normalizeBaseUrl(baseUrl);
  if (!normalizedBase) {
    return path;
  }
  if (normalizedBase.endsWith("/api") && path.startsWith("/api/")) {
    return `${normalizedBase}${path.slice("/api".length)}`;
  }
  return `${normalizedBase}${path}`;
}

function toWebSocketBaseUrl(baseUrl: string): string {
  if (!baseUrl) {
    return "";
  }
  if (baseUrl.startsWith("https://")) {
    return `wss://${baseUrl.slice("https://".length)}`;
  }
  if (baseUrl.startsWith("http://")) {
    return `ws://${baseUrl.slice("http://".length)}`;
  }
  return baseUrl;
}

function encodePath(path: string): string {
  return path
    .split("/")
    .map((part) => encodeURIComponent(part))
    .join("/");
}

type RouteDef = {
  method: "GET" | "POST";
  path: (args: Record<string, unknown>) => string;
};

const WIZARD_STATE_COMMANDS = new Set([
  "resolve_source",
  "parse_config",
  "detect_workspace",
  "confirm_workspace",
  "create_audit_session",
]);
const WIZARD_ID_STORAGE_KEY = "audit-agent:wizard-id";

function createWizardId(): string {
  if (
    typeof globalThis.crypto !== "undefined" &&
    typeof globalThis.crypto.randomUUID === "function"
  ) {
    return globalThis.crypto.randomUUID();
  }
  return `wiz-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
}

function getOrCreateWizardId(): string | null {
  if (typeof window === "undefined") {
    return null;
  }

  try {
    const existing = window.sessionStorage.getItem(WIZARD_ID_STORAGE_KEY);
    if (existing && existing.trim().length > 0) {
      return existing;
    }

    const wizardId = createWizardId();
    window.sessionStorage.setItem(WIZARD_ID_STORAGE_KEY, wizardId);
    return wizardId;
  } catch {
    return null;
  }
}

const COMMAND_ROUTES: Record<string, RouteDef> = {
  resolve_source: { method: "POST", path: () => "/api/source/resolve" },
  parse_config: { method: "POST", path: () => "/api/config/parse" },
  detect_workspace: { method: "POST", path: () => "/api/workspace/detect" },
  confirm_workspace: { method: "POST", path: () => "/api/workspace/confirm" },
  create_audit_session: { method: "POST", path: () => "/api/sessions" },
  list_audit_sessions: { method: "GET", path: () => "/api/sessions" },
  open_audit_session: {
    method: "GET",
    path: (args) => `/api/sessions/${encodeURIComponent(String(args.session_id ?? ""))}`,
  },
  get_project_tree: {
    method: "GET",
    path: (args) => `/api/sessions/${encodeURIComponent(String(args.session_id ?? ""))}/tree`,
  },
  read_source_file: {
    method: "GET",
    path: (args) =>
      `/api/sessions/${encodeURIComponent(String(args.session_id ?? ""))}/files/${encodePath(String(args.path ?? ""))}`,
  },
  load_file_graph: {
    method: "GET",
    path: (args) =>
      `/api/sessions/${encodeURIComponent(String(args.session_id ?? ""))}/graphs/file`,
  },
  load_feature_graph: {
    method: "GET",
    path: (args) =>
      `/api/sessions/${encodeURIComponent(String(args.session_id ?? ""))}/graphs/feature`,
  },
  load_dataflow_graph: {
    method: "GET",
    path: (args) => {
      const includeValues = args.include_values === true ? "?include_values=true" : "";
      return `/api/sessions/${encodeURIComponent(String(args.session_id ?? ""))}/graphs/dataflow${includeValues}`;
    },
  },
  load_symbol_graph: {
    method: "GET",
    path: (args) =>
      `/api/sessions/${encodeURIComponent(String(args.session_id ?? ""))}/graphs/symbol`,
  },
  load_security_overview: {
    method: "GET",
    path: (args) =>
      `/api/sessions/${encodeURIComponent(String(args.session_id ?? ""))}/security`,
  },
  tail_session_console: {
    method: "GET",
    path: (args) =>
      `/api/sessions/${encodeURIComponent(String(args.session_id ?? ""))}/console?limit=${encodeURIComponent(String(args.limit ?? 80))}`,
  },
  load_checklist_plan: {
    method: "GET",
    path: (args) =>
      `/api/sessions/${encodeURIComponent(String(args.session_id ?? ""))}/checklist`,
  },
  load_toolbench_context: {
    method: "POST",
    path: (args) =>
      `/api/sessions/${encodeURIComponent(String(args.session_id ?? ""))}/toolbench`,
  },
  load_review_queue: {
    method: "GET",
    path: (args) =>
      `/api/sessions/${encodeURIComponent(String(args.session_id ?? ""))}/review-queue`,
  },
  apply_review_decision: {
    method: "POST",
    path: (args) =>
      `/api/sessions/${encodeURIComponent(String(args.session_id ?? ""))}/review-decision`,
  },
  export_audit_yaml: {
    method: "POST",
    path: () => "/api/export/audit-yaml",
  },
  download_output: {
    method: "POST",
    path: () => "/api/output/download",
  },
  get_audit_manifest: {
    method: "GET",
    path: () => "/api/manifest",
  },
};

const GRAPH_TIMEOUT_MS = 10_000;
const GRAPH_COMMANDS = new Set([
  "load_file_graph",
  "load_feature_graph",
  "load_dataflow_graph",
  "load_symbol_graph",
]);

export class TauriTransport implements Transport {
  readonly kind: TransportKind = "tauri";

  invoke<T>(command: string, args: Record<string, unknown>): Promise<T> {
    const invoke = window.__TAURI__?.core?.invoke;
    if (!invoke) {
      return Promise.reject(new Error("Tauri invoke bridge is unavailable"));
    }
    return invoke<T>(command, args);
  }

  subscribe<T>(
    event: string,
    _sessionId: string,
    handler: (payload: T) => void
  ): () => void {
    const listen = window.__TAURI__?.event?.listen;
    if (!listen) {
      return () => undefined;
    }

    let disposed = false;
    let unlisten: (() => void) | null = null;
    listen<T>(event, (message) => handler(message.payload))
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
      unlisten?.();
    };
  }
}

type ApiErrorEnvelope = {
  error?: {
    code?: string;
    message?: string;
    status?: number;
  };
};

export class HttpTransport implements Transport {
  readonly kind: TransportKind = "http";
  private readonly baseUrl: string;
  private readonly wsBaseUrl: string;

  constructor(baseUrl: string) {
    this.baseUrl = normalizeBaseUrl(baseUrl);
    this.wsBaseUrl = toWebSocketBaseUrl(this.baseUrl);
  }

  async invoke<T>(command: string, args: Record<string, unknown>): Promise<T> {
    const route = COMMAND_ROUTES[command];
    if (!route) {
      throw new Error(`Unknown command: ${command}`);
    }

    const headers: Record<string, string> = { "Content-Type": "application/json" };
    if (WIZARD_STATE_COMMANDS.has(command)) {
      const wizardId = getOrCreateWizardId();
      if (wizardId) {
        headers["x-wizard-id"] = wizardId;
      }
    }

    const url = joinBaseUrlAndPath(this.baseUrl, route.path(args));
    const shouldTimeout = GRAPH_COMMANDS.has(command);
    const controller = shouldTimeout ? new AbortController() : null;
    const timeoutHandle = shouldTimeout
      ? globalThis.setTimeout(() => {
          controller?.abort();
        }, GRAPH_TIMEOUT_MS)
      : null;

    let response: Response;
    try {
      response = await fetch(url, {
        method: route.method,
        headers,
        body: route.method === "POST" ? JSON.stringify(args) : undefined,
        signal: controller?.signal,
      });
    } catch (error) {
      if (controller?.signal.aborted) {
        throw new Error("Request timed out after 10s");
      }
      throw error instanceof Error ? error : new Error("Network request failed");
    } finally {
      if (timeoutHandle !== null) {
        globalThis.clearTimeout(timeoutHandle);
      }
    }

    if (!response.ok) {
      let message = `HTTP ${response.status}`;
      try {
        const envelope = (await response.json()) as ApiErrorEnvelope;
        if (envelope.error?.message) {
          message = envelope.error.message;
        }
      } catch {
        // no-op
      }
      throw new Error(message);
    }

    if (response.status === 204) {
      return undefined as T;
    }

    return (await response.json()) as T;
  }

  subscribe<T>(
    _event: string,
    sessionId: string,
    handler: (payload: T) => void
  ): () => void {
    let closed = false;
    let activeSocket: WebSocket | null = null;
    let reconnectTimer: number | null = null;

    const connect = (): void => {
      if (closed) {
        return;
      }

      const socket = new WebSocket(
        `${this.wsBaseUrl}/api/sessions/${encodeURIComponent(sessionId)}/events`
      );
      activeSocket = socket;

      socket.onmessage = (message) => {
        if (typeof message.data !== "string") {
          return;
        }
        try {
          handler(JSON.parse(message.data) as T);
        } catch {
          // no-op
        }
      };

      socket.onclose = (event) => {
        if (closed || event.wasClean) {
          return;
        }
        reconnectTimer = window.setTimeout(() => {
          reconnectTimer = null;
          connect();
        }, 2_000);
      };
    };

    connect();

    return () => {
      closed = true;
      if (reconnectTimer !== null) {
        window.clearTimeout(reconnectTimer);
        reconnectTimer = null;
      }
      activeSocket?.close();
      activeSocket = null;
    };
  }
}

type TransportEnv = {
  VITE_TRANSPORT?: string;
  VITE_API_URL?: string;
  MODE?: string;
};

function readTransportEnv(): TransportEnv {
  const meta = import.meta as unknown as { env?: TransportEnv };
  return meta.env ?? {};
}

export function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && typeof window.__TAURI__?.core?.invoke === "function";
}

export function createTransport(env: Partial<TransportEnv> = readTransportEnv()): Transport {
  const mode =
    env.MODE ??
    (import.meta as unknown as { env?: { MODE?: string } }).env?.MODE;

  if (env.VITE_TRANSPORT === "tauri") {
    return new TauriTransport();
  }

  if (env.VITE_TRANSPORT === "http") {
    const configuredBase = env.VITE_API_URL?.trim();
    const runtimeDefault =
      typeof window !== "undefined"
        ? `${window.location.protocol}//${window.location.hostname}:3000`
        : "";
    return new HttpTransport(configuredBase && configuredBase.length > 0 ? configuredBase : runtimeDefault);
  }

  if (mode === "test") {
    return new TauriTransport();
  }

  // Safe default: in a regular browser session (including WSL web dev),
  // use HTTP transport unless the Tauri bridge is actually present.
  if (!isTauriRuntime()) {
    const runtimeDefault =
      typeof window !== "undefined"
        ? `${window.location.protocol}//${window.location.hostname}:3000`
        : "";
    return new HttpTransport(runtimeDefault);
  }

  return new TauriTransport();
}

let transportOverride: Transport | null = null;

export function getTransport(): Transport {
  return transportOverride ?? createTransport();
}

export function setTransportForTests(transport: Transport | null): void {
  transportOverride = transport;
}
