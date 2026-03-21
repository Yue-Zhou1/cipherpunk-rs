import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { HttpTransport, TauriTransport, createTransport } from "./transport";

describe("ipc transport", () => {
  beforeEach(() => {
    delete (window as typeof window & { __TAURI__?: unknown }).__TAURI__;
    window.sessionStorage.setItem("audit-agent:wizard-id", "wizard-test");
  });

  afterEach(() => {
    window.sessionStorage.removeItem("audit-agent:wizard-id");
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("maps resolve_source to POST /api/source/resolve", async () => {
    const fetchMock = vi.fn(async () =>
      new Response(JSON.stringify({ ok: true }), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      })
    );
    vi.stubGlobal("fetch", fetchMock);

    const transport = new HttpTransport("http://localhost:3000");
    const payload = { input: { kind: "git", value: "https://github.com/org/repo" } };
    const response = await transport.invoke<{ ok: boolean }>("resolve_source", payload);

    expect(fetchMock).toHaveBeenCalledWith("http://localhost:3000/api/source/resolve", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "x-wizard-id": "wizard-test",
      },
      body: JSON.stringify(payload),
    });
    expect(response.ok).toBe(true);
  });

  it("maps open_audit_session to GET /api/sessions/:id", async () => {
    const fetchMock = vi.fn(async () =>
      new Response(JSON.stringify({ sessionId: "sess-1" }), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      })
    );
    vi.stubGlobal("fetch", fetchMock);

    const transport = new HttpTransport("http://localhost:3000");
    await transport.invoke("open_audit_session", { session_id: "sess-1" });

    expect(fetchMock).toHaveBeenCalledWith("http://localhost:3000/api/sessions/sess-1", {
      method: "GET",
      headers: { "Content-Type": "application/json" },
      body: undefined,
    });
  });

  it("includes include_values query for dataflow graph route", async () => {
    const fetchMock = vi.fn(async () =>
      new Response(JSON.stringify({ nodes: [], edges: [] }), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      })
    );
    vi.stubGlobal("fetch", fetchMock);

    const transport = new HttpTransport("http://localhost:3000");
    await transport.invoke("load_dataflow_graph", {
      session_id: "sess-1",
      include_values: true,
    });

    expect(fetchMock).toHaveBeenCalledWith(
      "http://localhost:3000/api/sessions/sess-1/graphs/dataflow?include_values=true",
      {
        method: "GET",
        headers: { "Content-Type": "application/json" },
        body: undefined,
      }
    );
  });

  it("maps tail_session_console with limit query param", async () => {
    const fetchMock = vi.fn(async () =>
      new Response(JSON.stringify({ sessionId: "sess-1", entries: [] }), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      })
    );
    vi.stubGlobal("fetch", fetchMock);

    const transport = new HttpTransport("http://localhost:3000");
    await transport.invoke("tail_session_console", {
      session_id: "sess-1",
      limit: 25,
    });

    expect(fetchMock).toHaveBeenCalledWith(
      "http://localhost:3000/api/sessions/sess-1/console?limit=25",
      {
        method: "GET",
        headers: { "Content-Type": "application/json" },
        body: undefined,
      }
    );
  });

  it("maps download_output to POST /api/output/download", async () => {
    const fetchMock = vi.fn(async () =>
      new Response(JSON.stringify({ dest: "/tmp/findings.json" }), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      })
    );
    vi.stubGlobal("fetch", fetchMock);

    const transport = new HttpTransport("http://localhost:3000");
    const payload = {
      auditId: "audit-1",
      outputType: "findings_json",
      dest: "/tmp/findings.json",
    };
    await transport.invoke("download_output", payload);

    expect(fetchMock).toHaveBeenCalledWith("http://localhost:3000/api/output/download", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload),
    });
  });

  it("surfaces API error message from JSON envelope", async () => {
    const fetchMock = vi.fn(async () =>
      new Response(
        JSON.stringify({
          error: {
            code: "SESSION_NOT_FOUND",
            message: "No session with id 'sess-missing'",
            status: 404,
          },
        }),
        {
          status: 404,
          headers: { "Content-Type": "application/json" },
        }
      )
    );
    vi.stubGlobal("fetch", fetchMock);

    const transport = new HttpTransport("http://localhost:3000");
    await expect(
      transport.invoke("open_audit_session", { session_id: "sess-missing" })
    ).rejects.toThrow("No session with id 'sess-missing'");
  });

  it("throws when command route is unknown", async () => {
    const transport = new HttpTransport("http://localhost:3000");

    await expect(transport.invoke("unknown_command", {})).rejects.toThrow(
      "Unknown command: unknown_command"
    );
  });

  it("defaults to http transport in browser when VITE_TRANSPORT is unset", () => {
    const transport = createTransport({ MODE: "development" });
    expect(transport.kind).toBe("http");
  });

  it("defaults to tauri transport when bridge exists and VITE_TRANSPORT is unset", () => {
    (window as typeof window & { __TAURI__?: unknown }).__TAURI__ = {
      core: { invoke: vi.fn() },
    };

    const transport = createTransport({ MODE: "development" });
    expect(transport.kind).toBe("tauri");
  });

  it("defaults to tauri transport in test mode when VITE_TRANSPORT is unset", () => {
    const transport = createTransport({ MODE: "test" });
    expect(transport.kind).toBe("tauri");
  });

  it("honors explicit VITE_TRANSPORT=tauri even without bridge", () => {
    const transport = createTransport({ VITE_TRANSPORT: "tauri" });
    expect(transport.kind).toBe("tauri");
  });

  it("invokes tauri command bridge when available", async () => {
    const invokeCalls: Array<{
      command: string;
      args: Record<string, unknown> | undefined;
    }> = [];
    const invoke = async <T>(
      command: string,
      args?: Record<string, unknown>
    ): Promise<T> => {
      invokeCalls.push({ command, args });
      return { auditId: "audit-1" } as T;
    };
    (window as typeof window & { __TAURI__?: unknown }).__TAURI__ = {
      core: { invoke },
    };

    const transport = new TauriTransport();
    const response = await transport.invoke<{ auditId: string }>("confirm_workspace", {
      decisions: { confirmed: true, ambiguousCrates: {} },
    });

    expect(invokeCalls).toEqual([
      {
        command: "confirm_workspace",
        args: { decisions: { confirmed: true, ambiguousCrates: {} } },
      },
    ]);
    expect(response.auditId).toBe("audit-1");
  });
});
