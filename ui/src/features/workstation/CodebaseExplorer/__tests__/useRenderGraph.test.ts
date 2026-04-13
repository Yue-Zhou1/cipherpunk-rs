import { renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { useRenderGraph } from "../hooks/useRenderGraph";

const BASE_GRAPH = {
  nodes: [
    { id: "f1", label: "main.rs", kind: "file" as const },
    { id: "s1", label: "do_thing", kind: "function" as const, filePath: "main.rs", line: 10 },
  ],
  edges: [
    { from: "f1", to: "s1", relation: "contains" as const },
    { from: "s1", to: "s1", relation: "calls" as const },
  ],
};

const BASE_CTX = {
  graph: BASE_GRAPH,
  focusedNodeId: null,
  upstreamIds: new Set<string>(),
  downstreamIds: new Set<string>(),
  stateKind: "overview" as const,
  neighborhoodResult: null,
  matchingNodeIds: null,
};

describe("useRenderGraph", () => {
  it("produces one RenderNode per graph node", () => {
    const positions = new Map([
      ["f1", { x: 0, y: 0 }],
      ["s1", { x: 200, y: 100 }],
    ]);

    const { result } = renderHook(() => useRenderGraph(BASE_CTX, positions));
    expect(result.current.nodes).toHaveLength(2);
  });

  it("excludes contains edges from render edges", () => {
    const { result } = renderHook(() => useRenderGraph(BASE_CTX, new Map()));
    const relations = result.current.edges.map((edge) => edge.relation);
    expect(relations).not.toContain("contains");
  });

  it("dims non-ego nodes to 0.08 opacity in focus mode", () => {
    const positions = new Map([
      ["f1", { x: 0, y: 0 }],
      ["s1", { x: 200, y: 100 }],
    ]);
    const ctx = {
      ...BASE_CTX,
      focusedNodeId: "s1",
      stateKind: "focus" as const,
    };

    const { result } = renderHook(() => useRenderGraph(ctx, positions));
    const fileNode = result.current.nodes.find((node) => node.id === "f1");
    const symbolNode = result.current.nodes.find((node) => node.id === "s1");
    expect(fileNode?.opacity).toBe(0.08);
    expect(symbolNode?.opacity).toBe(1);
  });

  it("sets focused node border to white", () => {
    const ctx = { ...BASE_CTX, focusedNodeId: "s1", stateKind: "focus" as const };
    const { result } = renderHook(() => useRenderGraph(ctx, new Map()));
    const symbolNode = result.current.nodes.find((node) => node.id === "s1");
    expect(symbolNode?.borderColor).toBe(0xffffff);
  });

  it("uses dashed style for parameter_flow and return_flow edges", () => {
    const graph = {
      ...BASE_GRAPH,
      edges: [
        { from: "f1", to: "s1", relation: "parameter_flow" as const },
        { from: "s1", to: "f1", relation: "return_flow" as const },
      ],
    };

    const { result } = renderHook(() =>
      useRenderGraph({ ...BASE_CTX, graph }, new Map())
    );

    expect(result.current.edges.every((edge) => edge.dashed)).toBe(true);
  });
});
