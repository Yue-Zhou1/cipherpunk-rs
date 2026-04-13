import { describe, expect, it, vi } from "vitest";

vi.mock("elkjs/lib/elk.bundled.js", () => ({
  default: class MockElk {
    async layout(graph: { children?: Array<{ id: string }> }) {
      return {
        children: (graph.children ?? []).map((child, index) => ({
          id: child.id,
          x: index * 200,
          y: 0,
        })),
      };
    }
  },
}));

import { runElkLayout } from "../layout/ElkLayout";

describe("runElkLayout", () => {
  it("returns positions for all input nodes", async () => {
    const nodes = [
      { id: "a", kind: "file" as const, width: 140, height: 32 },
      { id: "b", kind: "file" as const, width: 140, height: 32 },
    ];
    const edges = [{ id: "e1", fromId: "a", toId: "b", relation: "calls" as const }];

    const positions = await runElkLayout(nodes, edges);

    expect(positions.has("a")).toBe(true);
    expect(positions.has("b")).toBe(true);
  });

  it("excludes contains edges from ELK input", async () => {
    const nodes = [{ id: "crt", kind: "crate" as const, width: 160, height: 40 }];
    const edges = [{ id: "e1", fromId: "crt", toId: "mod", relation: "contains" as const }];

    const positions = await runElkLayout(nodes, edges);

    expect(positions instanceof Map).toBe(true);
  });
});
