import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it, beforeEach, vi } from "vitest";

import { useAdaptiveThresholds } from "../hooks/useAdaptiveThresholds";
import { useDepthControl } from "../hooks/useDepthControl";
import { useFocusContext } from "../hooks/useFocusContext";
import { useTrace } from "../hooks/useTrace";
import { useUnifiedGraph } from "../hooks/useUnifiedGraph";
import type { ExplorerGraph } from "../types";

const mockTransportSubscribe = vi.fn();
const mockTransportSubscribers: Array<(payload: { event?: string }) => void> = [];

function emitTransportEvent(payload: { event?: string }): void {
  for (const subscriber of mockTransportSubscribers) {
    subscriber(payload);
  }
}

vi.mock("../../../../ipc/commands", () => ({
  loadExplorerGraph: vi.fn(),
}));

vi.mock("../../../../ipc/transport", () => ({
  getTransport: () => ({
    subscribe: mockTransportSubscribe.mockImplementation(
      (
        _event: string,
        _sessionId: string,
        handler: (payload: { event?: string }) => void
      ) => {
        mockTransportSubscribers.push(handler);
        return vi.fn();
      }
    ),
  }),
}));

import { loadExplorerGraph } from "../../../../ipc/commands";

const mockedLoadExplorerGraph = vi.mocked(loadExplorerGraph);

function makeTestGraph(overrides?: Partial<ExplorerGraph>): ExplorerGraph {
  return {
    nodes:
      overrides?.nodes ??
      [
        { id: "crt_1", label: "mycrate", kind: "crate", childCount: 1 },
        { id: "mod_1", label: "src", kind: "module", childCount: 2 },
        { id: "fil_1", label: "lib.rs", kind: "file", filePath: "mycrate/src/lib.rs" },
        {
          id: "sym_1",
          label: "verify",
          kind: "function",
          filePath: "mycrate/src/lib.rs",
          line: 10,
          signature: {
            parameters: [{ name: "msg", typeAnnotation: "&[u8]", position: 0 }],
            returnType: "bool",
          },
        },
        {
          id: "sym_2",
          label: "hash",
          kind: "function",
          filePath: "mycrate/src/lib.rs",
          line: 21,
        },
      ],
    edges:
      overrides?.edges ??
      [
        { from: "crt_1", to: "mod_1", relation: "contains" },
        { from: "mod_1", to: "fil_1", relation: "contains" },
        { from: "fil_1", to: "sym_1", relation: "contains" },
        { from: "fil_1", to: "sym_2", relation: "contains" },
        { from: "sym_1", to: "sym_2", relation: "calls" },
        {
          from: "sym_2",
          to: "sym_1",
          relation: "parameter_flow",
          parameterName: "msg",
          parameterPosition: 0,
        },
        { from: "sym_1", to: "sym_2", relation: "return_flow" },
      ],
  };
}

describe("useDepthControl", () => {
  it("defaults to depth 2", () => {
    const { result } = renderHook(() => useDepthControl());
    expect(result.current.depth).toBe(2);
  });

  it("clamps depth to range 1-10", () => {
    const { result } = renderHook(() => useDepthControl());
    act(() => result.current.setDepth(0));
    expect(result.current.depth).toBe(1);
    act(() => result.current.setDepth(15));
    expect(result.current.depth).toBe(10);
  });
});

describe("useAdaptiveThresholds", () => {
  it("resolves to files for small graphs", () => {
    const { result } = renderHook(() => useAdaptiveThresholds(15));
    expect(result.current.resolvedGranularity).toBe("files");
  });

  it("resolves to modules for medium graphs", () => {
    const { result } = renderHook(() => useAdaptiveThresholds(50));
    expect(result.current.resolvedGranularity).toBe("modules");
  });

  it("resolves to crates for large graphs", () => {
    const { result } = renderHook(() => useAdaptiveThresholds(200));
    expect(result.current.resolvedGranularity).toBe("crates");
  });
});

describe("useUnifiedGraph", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockTransportSubscribers.length = 0;
  });

  it("calls loadExplorerGraph with overview on mount", async () => {
    mockedLoadExplorerGraph.mockResolvedValue({
      sessionId: "s1",
      nodes: [{ id: "crt_1", label: "mycrate", kind: "crate", childCount: 1 }],
      edges: [],
    });

    const { result } = renderHook(() => useUnifiedGraph("s1"));

    expect(result.current.isLoading).toBe(true);
    await waitFor(() => expect(result.current.isLoading).toBe(false));

    expect(mockedLoadExplorerGraph).toHaveBeenCalledWith("s1", "overview");
    expect(result.current.graph.nodes).toHaveLength(1);
    expect(result.current.nodeMap.get("crt_1")).toBeDefined();
    expect(result.current.error).toBeNull();
  });

  it("sets error on API failure", async () => {
    mockedLoadExplorerGraph.mockRejectedValue(new Error("Network error"));

    const { result } = renderHook(() => useUnifiedGraph("s1"));
    await waitFor(() => expect(result.current.isLoading).toBe(false));

    expect(result.current.error).toBe("Network error");
    expect(result.current.graph.nodes).toHaveLength(0);
  });

  it("expandCluster merges without duplicates", async () => {
    mockedLoadExplorerGraph
      .mockResolvedValueOnce({
        sessionId: "s1",
        nodes: [{ id: "crt_1", label: "mycrate", kind: "crate", childCount: 1 }],
        edges: [],
      })
      .mockResolvedValueOnce({
        sessionId: "s1",
        nodes: [
          { id: "crt_1", label: "mycrate", kind: "crate" },
          { id: "fil_1", label: "lib.rs", kind: "file" },
        ],
        edges: [{ from: "crt_1", to: "fil_1", relation: "contains" }],
      });

    const { result } = renderHook(() => useUnifiedGraph("s1"));
    await waitFor(() => expect(result.current.isLoading).toBe(false));

    act(() => result.current.expandCluster("crt_1"));

    await waitFor(() => expect(result.current.graph.nodes).toHaveLength(2));
    expect(result.current.graph.nodes.filter((node) => node.id === "crt_1")).toHaveLength(1);
  });

  it("keeps distinct parameter_flow edges when merging cluster results", async () => {
    mockedLoadExplorerGraph
      .mockResolvedValueOnce({
        sessionId: "s1",
        nodes: [
          { id: "crt_1", label: "mycrate", kind: "crate", childCount: 1 },
          { id: "sym_src", label: "source", kind: "function" },
          { id: "sym_dst", label: "target", kind: "function" },
        ],
        edges: [
          {
            from: "sym_src",
            to: "sym_dst",
            relation: "parameter_flow",
            parameterName: "msg",
            parameterPosition: 0,
          },
        ],
      })
      .mockResolvedValueOnce({
        sessionId: "s1",
        nodes: [],
        edges: [
          {
            from: "sym_src",
            to: "sym_dst",
            relation: "parameter_flow",
            parameterName: "sig",
            parameterPosition: 1,
          },
        ],
      });

    const { result } = renderHook(() => useUnifiedGraph("s1"));
    await waitFor(() => expect(result.current.isLoading).toBe(false));

    act(() => result.current.expandCluster("crt_1"));

    await waitFor(() =>
      expect(result.current.graph.edges.filter((edge) => edge.relation === "parameter_flow")).toHaveLength(2)
    );
    const parameterNames = result.current.graph.edges
      .filter((edge) => edge.relation === "parameter_flow")
      .map((edge) => edge.parameterName);
    expect(new Set(parameterNames)).toEqual(new Set(["msg", "sig"]));
  });

  it("surfaces stale state when stale event arrives during loading", async () => {
    let resolveOverview: ((value: {
      sessionId: string;
      nodes: Array<{ id: string; label: string; kind: string }>;
      edges: unknown[];
    }) => void) | null = null;
    const overviewPromise = new Promise<{
      sessionId: string;
      nodes: Array<{ id: string; label: string; kind: string }>;
      edges: unknown[];
    }>((resolve) => {
      resolveOverview = resolve;
    });
    mockedLoadExplorerGraph.mockReturnValueOnce(overviewPromise);

    const { result } = renderHook(() => useUnifiedGraph("s1"));
    expect(result.current.isLoading).toBe(true);

    act(() => {
      emitTransportEvent({ event: "explorer_graph_stale" });
    });

    act(() => {
      resolveOverview?.({
        sessionId: "s1",
        nodes: [{ id: "crt_1", label: "mycrate", kind: "crate" }],
        edges: [],
      });
    });

    await waitFor(() => expect(result.current.isLoading).toBe(false));
    expect(result.current.isStale).toBe(true);
  });
});

describe("useFocusContext", () => {
  it("starts in overview state with no focus", () => {
    const { result } = renderHook(() => useFocusContext(makeTestGraph(), 2));
    expect(result.current.stateKind).toBe("overview");
    expect(result.current.focusedNodeId).toBeNull();
  });

  it("focusing a node transitions to focus state", () => {
    const graph = makeTestGraph();
    const { result } = renderHook(() => useFocusContext(graph, 2));

    act(() => result.current.focusNode("sym_1"));

    expect(result.current.stateKind).toBe("focus");
    expect(result.current.focusedNodeId).toBe("sym_1");
    expect(result.current.downstreamIds.has("sym_2")).toBe(true);
  });

  it("depth change recomputes neighbors", () => {
    const graph = makeTestGraph({
      edges: [
        { from: "sym_1", to: "sym_2", relation: "calls" },
        { from: "sym_2", to: "sym_3", relation: "calls" },
      ],
      nodes: [
        { id: "sym_1", label: "a", kind: "function" },
        { id: "sym_2", label: "b", kind: "function" },
        { id: "sym_3", label: "c", kind: "function" },
      ],
    });

    const { result, rerender } = renderHook(
      ({ depth }) => useFocusContext(graph, depth),
      { initialProps: { depth: 1 } }
    );

    act(() => result.current.focusNode("sym_1"));
    const depthOneCount = result.current.downstreamIds.size;

    rerender({ depth: 2 });
    const depthTwoCount = result.current.downstreamIds.size;

    expect(depthTwoCount).toBeGreaterThanOrEqual(depthOneCount);
  });
});

describe("useTrace", () => {
  it("starts with no trace", () => {
    const { result } = renderHook(() => useTrace(makeTestGraph(), null));
    expect(result.current.traceResult).toBeNull();
  });

  it("tracing a parameter computes upstream path", () => {
    const graph = makeTestGraph();
    const { result } = renderHook(() => useTrace(graph, "sym_1"));

    act(() => result.current.traceParameter("msg"));

    expect(result.current.traceResult).not.toBeNull();
    expect(result.current.traceResult?.direction).toBe("upstream");
    expect(result.current.traceResult?.parameterName).toBe("msg");
  });

  it("clearTrace resets result", () => {
    const graph = makeTestGraph();
    const { result } = renderHook(() => useTrace(graph, "sym_1"));

    act(() => result.current.traceParameter("msg"));
    act(() => result.current.clearTrace());

    expect(result.current.traceResult).toBeNull();
  });
});
