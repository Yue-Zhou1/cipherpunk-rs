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

  it("returns already-entry-point message for node without callers", () => {
    const entryGraph: ExplorerGraph = {
      nodes: [{ id: "entry", label: "entry_fn", kind: "function" }],
      edges: [],
    };
    const entryNodeMap = new Map([["entry", { label: "entry_fn" }]]);
    const { result } = renderHook(() =>
      useTraceAlgorithm(entryGraph, entryNodeMap)
    );

    act(() => result.current.traceToEntryPoint("entry"));

    expect(result.current.traceResult?.banner).toContain("already an entry point");
  });

  it("returns no-entry-found message for cycle with no entry node", () => {
    const cyclicGraph: ExplorerGraph = {
      nodes: [
        { id: "a", label: "a", kind: "function" },
        { id: "b", label: "b", kind: "function" },
      ],
      edges: [
        { from: "a", to: "b", relation: "calls" },
        { from: "b", to: "a", relation: "calls" },
      ],
    };
    const cyclicNodeMap = new Map([
      ["a", { label: "a" }],
      ["b", { label: "b" }],
    ]);
    const { result } = renderHook(() =>
      useTraceAlgorithm(cyclicGraph, cyclicNodeMap)
    );

    act(() => result.current.traceToEntryPoint("a"));

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
