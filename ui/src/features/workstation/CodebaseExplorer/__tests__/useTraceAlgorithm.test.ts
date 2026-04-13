import { act, renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { useTraceAlgorithm } from "../hooks/useTraceAlgorithm";
import type { ExplorerGraph } from "../types";

const GRAPH: ExplorerGraph = {
  nodes: [
    { id: "net", label: "recv_message", kind: "function" },
    { id: "val", label: "validate", kind: "function" },
    { id: "cry", label: "verify_sig", kind: "function" },
  ],
  edges: [
    { from: "net", to: "val", relation: "calls" },
    { from: "val", to: "cry", relation: "calls" },
    { from: "net", to: "cry", relation: "parameter_flow", parameterName: "msg" },
  ],
};

const NODE_MAP = new Map([
  ["net", { label: "recv_message" }],
  ["val", { label: "validate" }],
  ["cry", { label: "verify_sig" }],
]);

describe("useTraceAlgorithm - traceToEntryPoint", () => {
  it("finds path from entry to target", () => {
    const { result } = renderHook(() => useTraceAlgorithm(GRAPH, NODE_MAP));

    act(() => result.current.traceToEntryPoint("cry"));

    expect(result.current.traceResult?.pathNodeIds).toEqual(["net", "val", "cry"]);
  });

  it("banner includes hop count", () => {
    const { result } = renderHook(() => useTraceAlgorithm(GRAPH, NODE_MAP));

    act(() => result.current.traceToEntryPoint("cry"));

    expect(result.current.traceResult?.banner).toContain("2 hops");
  });

  it("returns no-entry-found message for isolated node", () => {
    const isolatedGraph: ExplorerGraph = {
      nodes: [{ id: "iso", label: "iso", kind: "function" }],
      edges: [],
    };
    const isolatedNodeMap = new Map([["iso", { label: "iso" }]]);
    const { result } = renderHook(() =>
      useTraceAlgorithm(isolatedGraph, isolatedNodeMap)
    );

    act(() => result.current.traceToEntryPoint("iso"));

    expect(result.current.traceResult?.banner).toContain("No entry point found");
  });

  it("clearTrace resets result to null", () => {
    const { result } = renderHook(() => useTraceAlgorithm(GRAPH, NODE_MAP));

    act(() => result.current.traceToEntryPoint("cry"));
    act(() => result.current.clearTrace());

    expect(result.current.traceResult).toBeNull();
  });
});

describe("useTraceAlgorithm - traceDataflowForNode", () => {
  it("identifies dataflow edges for the selected node", () => {
    const { result } = renderHook(() => useTraceAlgorithm(GRAPH, NODE_MAP));

    act(() => result.current.traceDataflowForNode("cry"));

    expect(result.current.traceResult?.kind).toBe("dataflow");
    expect(result.current.traceResult?.pathEdgeIds).toEqual(
      new Set(["net::cry::parameter_flow"])
    );
  });
});
