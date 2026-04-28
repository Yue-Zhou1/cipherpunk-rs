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

  it("dims non-path nodes and highlights trace path nodes", () => {
    const traceResult = {
      kind: "attack_path" as const,
      pathNodeIds: ["s1"],
      pathEdgeIds: new Set<string>(),
      banner: "Attack path",
    };
    const { result } = renderHook(() =>
      useRenderGraph(BASE_CTX, new Map(), traceResult)
    );

    const fileNode = result.current.nodes.find((node) => node.id === "f1");
    const symbolNode = result.current.nodes.find((node) => node.id === "s1");
    expect(fileNode?.opacity).toBe(0.06);
    expect(symbolNode?.opacity).toBe(1);
    expect(symbolNode?.borderColor).toBe(0xf59e0b);
  });

  it("colors attack-path edges amber and fades non-path edges", () => {
    const traceResult = {
      kind: "attack_path" as const,
      pathNodeIds: ["s1"],
      pathEdgeIds: new Set(["s1::s1::calls"]),
      banner: "Attack path",
    };
    const { result } = renderHook(() =>
      useRenderGraph(BASE_CTX, new Map(), traceResult)
    );

    expect(result.current.edges).toHaveLength(1);
    expect(result.current.edges[0]?.color).toBe(0xf59e0b);
    expect(result.current.edges[0]?.opacity).toBe(1);
  });

  it("keeps dataflow edge colors when traced in dataflow mode", () => {
    const graph = {
      ...BASE_GRAPH,
      edges: [{ from: "f1", to: "s1", relation: "parameter_flow" as const }],
    };
    const traceResult = {
      kind: "dataflow" as const,
      pathNodeIds: ["s1"],
      pathEdgeIds: new Set(["f1::s1::parameter_flow"]),
      banner: "Dataflow",
    };

    const { result } = renderHook(() =>
      useRenderGraph({ ...BASE_CTX, graph }, new Map(), traceResult)
    );

    expect(result.current.edges).toHaveLength(1);
    expect(result.current.edges[0]?.color).toBe(0xa78bfa);
    expect(result.current.edges[0]?.opacity).toBe(1);
  });

  it("renders invokes_macro edges with a visible (non-black) color", () => {
    const m1 = { id: "m1", label: "assert!", kind: "macro_call" as const };
    const graph = {
      nodes: [...BASE_GRAPH.nodes, m1],
      edges: [
        { from: "s1", to: "m1", relation: "invokes_macro" as const },
      ],
    };

    const { result } = renderHook(() =>
      useRenderGraph({ ...BASE_CTX, graph }, new Map())
    );

    const macroEdge = result.current.edges.find((e) => e.relation === "invokes_macro");
    expect(macroEdge).toBeDefined();
    // 0x000000 (pure black) is invisible on the dark canvas background
    expect(macroEdge?.color).not.toBe(0x000000);
  });

  it("does not recompute when ctx fields are unchanged but ctx object reference changes", () => {
    const positions = new Map([["f1", { x: 0, y: 0 }]]);
    let renderCount = 0;

    // Wrap useRenderGraph to track how many times the memo actually recomputes
    // We detect this by checking that renderGraph reference is stable across renders
    // when none of the semantic inputs changed.
    const { result, rerender } = renderHook(
      ({ ctx }: { ctx: typeof BASE_CTX }) => {
        renderCount++;
        return useRenderGraph(ctx, positions);
      },
      { initialProps: { ctx: BASE_CTX } }
    );

    const firstResult = result.current;
    // Rerender with a new object reference but identical semantic content
    rerender({ ctx: { ...BASE_CTX } });

    // The renderGraph reference must be stable — no recomputation
    expect(result.current).toBe(firstResult);
  });
});
