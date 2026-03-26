import { describe, expect, it } from "vitest";

import { largeFixture, mediumFixture, smallFixture } from "../fixtures/mockGraph";
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
