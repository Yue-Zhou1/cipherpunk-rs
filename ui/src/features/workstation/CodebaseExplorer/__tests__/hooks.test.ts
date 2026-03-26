import { act, renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { largeFixture, mediumFixture, smallFixture } from "../fixtures/mockGraph";
import { useAdaptiveThresholds } from "../hooks/useAdaptiveThresholds";
import { useDepthControl } from "../hooks/useDepthControl";
import { useUnifiedGraph } from "../hooks/useUnifiedGraph";
import type { ExplorerGraph } from "../types";

function fileCount(graph: ExplorerGraph): number {
  return graph.nodes.filter((node) => node.kind === "file").length;
}

describe("fixture data", () => {
  it("small fixture has fewer than 30 file nodes", () => {
    expect(fileCount(smallFixture)).toBeLessThan(30);
    expect(fileCount(smallFixture)).toBeGreaterThan(0);
  });

  it("medium fixture has 30-150 file nodes", () => {
    const count = fileCount(mediumFixture);
    expect(count).toBeGreaterThanOrEqual(30);
    expect(count).toBeLessThanOrEqual(150);
  });

  it("large fixture has more than 150 file nodes", () => {
    expect(fileCount(largeFixture)).toBeGreaterThan(150);
  });

  it("medium fixture has symbol nodes with signatures", () => {
    const withSig = mediumFixture.nodes.filter((node) => node.signature);
    expect(withSig.length).toBeGreaterThan(10);
    const sig = withSig[0].signature!;
    expect(sig.parameters.length).toBeGreaterThan(0);
    expect(sig.parameters[0].name).toBeTruthy();
  });

  it("medium fixture has parameter_flow edges with parameterName", () => {
    const paramFlows = mediumFixture.edges.filter((edge) => edge.relation === "parameter_flow");
    expect(paramFlows.length).toBeGreaterThan(0);
    expect(paramFlows[0].parameterName).toBeTruthy();
  });

  it("all edge references point to existing nodes", () => {
    for (const fixture of [smallFixture, mediumFixture, largeFixture]) {
      const nodeIds = new Set(fixture.nodes.map((node) => node.id));
      for (const edge of fixture.edges) {
        expect(nodeIds.has(edge.from), `missing node ${edge.from}`).toBe(true);
        expect(nodeIds.has(edge.to), `missing node ${edge.to}`).toBe(true);
      }
    }
  });
});

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

  it("accepts valid depth values", () => {
    const { result } = renderHook(() => useDepthControl());
    act(() => result.current.setDepth(5));
    expect(result.current.depth).toBe(5);
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

  it("manual override bypasses auto", () => {
    const { result } = renderHook(() => useAdaptiveThresholds(200));
    act(() => result.current.setGranularity("files"));
    expect(result.current.resolvedGranularity).toBe("files");
  });

  it("custom thresholds change resolution", () => {
    const { result } = renderHook(() => useAdaptiveThresholds(50));
    expect(result.current.resolvedGranularity).toBe("modules");
    act(() => result.current.setThresholds({ small: 100, large: 200 }));
    expect(result.current.resolvedGranularity).toBe("files");
  });
});

describe("useUnifiedGraph", () => {
  it("returns medium fixture by default", () => {
    const { result } = renderHook(() => useUnifiedGraph());
    expect(result.current.graph.nodes.length).toBeGreaterThan(0);
    expect(result.current.graph.edges.length).toBeGreaterThan(0);
  });

  it("can switch dataset size", () => {
    const { result } = renderHook(() => useUnifiedGraph("small"));
    const smallCount = result.current.graph.nodes.filter((node) => node.kind === "file").length;
    expect(smallCount).toBeLessThan(30);
  });
});
