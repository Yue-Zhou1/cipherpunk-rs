import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  chooseSavePath,
  confirmWorkspace,
  detectWorkspace,
  downloadOutput,
  getAuditManifest,
  isTauriRuntime,
  loadExplorerGraph,
  resolveSource,
  subscribeExecutionUpdates,
} from "./commands";
import { setTransportForTests, type Transport } from "./transport";

describe("ipc commands", () => {
  beforeEach(() => {
    delete (window as typeof window & { __TAURI__?: unknown }).__TAURI__;
  });

  afterEach(() => {
    setTransportForTests(null);
    vi.restoreAllMocks();
    vi.useRealTimers();
  });

  it("reports non-tauri runtime when invoke bridge is missing", () => {
    expect(isTauriRuntime()).toBe(false);
  });

  it("falls back to mock confirmWorkspace response in browser mode", async () => {
    const response = await confirmWorkspace({
      confirmed: true,
      ambiguousCrates: { "bridge-adapter": true },
    });

    expect(response.auditId).toContain("audit-");
  });

  it("delegates command invocation to the configured transport", async () => {
    const invokeCalls: Array<{
      command: string;
      args: Record<string, unknown>;
    }> = [];
    const invoke: Transport["invoke"] = async <T>(
      command: string,
      args: Record<string, unknown>
    ): Promise<T> => {
      invokeCalls.push({ command, args });
      return { auditId: "audit-from-transport" } as T;
    };
    const transport: Transport = {
      kind: "http",
      invoke,
      subscribe: () => () => undefined,
    };
    setTransportForTests(transport);

    const response = await confirmWorkspace({
      confirmed: true,
      ambiguousCrates: { "bridge-adapter": false },
    });

    expect(invokeCalls).toEqual([
      {
        command: "confirm_workspace",
        args: { decisions: { confirmed: true, ambiguousCrates: { "bridge-adapter": false } } },
      },
    ]);
    expect(response.auditId).toBe("audit-from-transport");
  });

  it("uses tauri invoke bridge when available", async () => {
    const invoke = vi.fn(async () => ({ auditId: "audit-from-tauri" }));
    (window as any).__TAURI__ = {
      core: { invoke },
    };

    const response = await confirmWorkspace({
      confirmed: true,
      ambiguousCrates: { "bridge-adapter": false },
    });

    expect(isTauriRuntime()).toBe(true);
    expect(invoke).toHaveBeenCalledWith("confirm_workspace", {
      decisions: { confirmed: true, ambiguousCrates: { "bridge-adapter": false } },
    });
    expect(response.auditId).toBe("audit-from-tauri");
  });

  it("invokes load_explorer_graph through transport with optional depth/cluster", async () => {
    const invoke = vi.fn(async <T>() => ({ sessionId: "sess-1", nodes: [], edges: [] } as T));
    const transport: Transport = {
      kind: "http",
      invoke,
      subscribe: () => () => undefined,
    };
    setTransportForTests(transport);

    await loadExplorerGraph("sess-1", "overview");
    await loadExplorerGraph("sess-1", undefined, "crt_1");

    expect(invoke).toHaveBeenNthCalledWith(1, "load_explorer_graph", {
      session_id: "sess-1",
      depth: "overview",
      cluster: undefined,
    });
    expect(invoke).toHaveBeenNthCalledWith(2, "load_explorer_graph", {
      session_id: "sess-1",
      depth: undefined,
      cluster: "crt_1",
    });
  });

  it("applies tiered loadExplorerGraph request timeouts", async () => {
    vi.useFakeTimers();
    const transport: Transport = {
      kind: "http",
      invoke: async <T>() => new Promise<T>(() => undefined),
      subscribe: () => () => undefined,
    };
    setTransportForTests(transport);

    const overviewRequest = loadExplorerGraph("sess-1", "overview");
    const overviewAssertion = expect(overviewRequest).rejects.toThrow("Request timed out after 3s");
    await vi.advanceTimersByTimeAsync(3_000);
    await overviewAssertion;

    const clusterRequest = loadExplorerGraph("sess-1", undefined, "crt_1");
    const clusterAssertion = expect(clusterRequest).rejects.toThrow("Request timed out after 5s");
    await vi.advanceTimersByTimeAsync(5_000);
    await clusterAssertion;

    const fullRequest = loadExplorerGraph("sess-1", "full");
    const fullAssertion = expect(fullRequest).rejects.toThrow("Request timed out after 15s");
    await vi.advanceTimersByTimeAsync(15_000);
    await fullAssertion;
  });

  it("uses transport subscription when http transport is configured", () => {
    const unsubscribe = vi.fn();
    const subscribe = vi.fn(() => unsubscribe);
    const transport: Transport = {
      kind: "http",
      invoke: async <T>() => ({} as T),
      subscribe,
    };
    setTransportForTests(transport);

    const stop = subscribeExecutionUpdates("audit-123", () => undefined);

    expect(subscribe).toHaveBeenCalledTimes(1);
    expect(subscribe).toHaveBeenCalledWith(
      "audit_execution_update",
      "audit-123",
      expect.any(Function)
    );
    stop();
    expect(unsubscribe).toHaveBeenCalledTimes(1);
  });

  it("returns download destination through fallback in browser mode", async () => {
    const response = await downloadOutput(
      "audit-20260305-a1b2c3d4",
      "findings_json",
      "/tmp/findings.json"
    );
    expect(response.dest).toBe("/tmp/findings.json");
  });

  it("falls back for resolveSource and detectWorkspace in browser mode", async () => {
    const source = await resolveSource({
      kind: "git",
      value: "https://github.com/org/repo",
      commitOrRef: "a1b2c3d4ef5678",
    });
    expect(source.commitHash).toBe("a1b2c3d4ef5678");
    expect(source.branchResolutionBanner).toContain("Resolved to SHA");

    const workspace = await detectWorkspace();
    expect(workspace.crateCount).toBe(3);
    expect(workspace.frameworks.length).toBeGreaterThan(0);
  });

  it("returns a fallback audit manifest in browser mode", async () => {
    const manifest = await getAuditManifest();
    expect(manifest.auditId).toBe("audit-20260305-a1b2c3d4");
    expect(manifest.riskScore).toBe(65);
  });

  it("uses save dialog when available", async () => {
    const save = vi.fn(async () => "/tmp/chosen.json");
    (window as any).__TAURI__ = {
      dialog: { save },
    };

    const selected = await chooseSavePath("findings.json");
    expect(selected).toBe("/tmp/chosen.json");
    expect(save).toHaveBeenCalledWith({ defaultPath: "findings.json" });
  });
});
