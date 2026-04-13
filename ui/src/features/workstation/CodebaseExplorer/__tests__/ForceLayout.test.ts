import { describe, expect, it } from "vitest";

import { runForceLayout } from "../layout/ForceLayout";

describe("runForceLayout", () => {
  it("returns a position for every input node", () => {
    const positions = new Map([
      ["center", { x: 400, y: 300 }],
      ["up1", { x: 100, y: 300 }],
      ["down1", { x: 700, y: 300 }],
    ]);

    const result = runForceLayout(positions, {
      focusedNodeId: "center",
      upstreamIds: new Set(["up1"]),
      downstreamIds: new Set(["down1"]),
      center: { x: 400, y: 300 },
      columnWidth: 300,
    });

    expect(result.size).toBe(3);
  });

  it("keeps focused node at center position", () => {
    const positions = new Map([["center", { x: 0, y: 0 }]]);
    const result = runForceLayout(positions, {
      focusedNodeId: "center",
      upstreamIds: new Set(),
      downstreamIds: new Set(),
      center: { x: 400, y: 300 },
      columnWidth: 300,
    });

    const centerPos = result.get("center");
    expect(centerPos?.x).toBeCloseTo(400, 0);
    expect(centerPos?.y).toBeCloseTo(300, 0);
  });
});
