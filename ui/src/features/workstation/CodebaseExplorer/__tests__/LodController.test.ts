import { describe, expect, it } from "vitest";

import { lodConfig, zoomToLod } from "../pixi/LodController";

describe("zoomToLod", () => {
  it("returns overview below 0.4", () => {
    expect(zoomToLod(0.3)).toBe("overview");
  });

  it("returns navigation between 0.4 and 0.8", () => {
    expect(zoomToLod(0.6)).toBe("navigation");
  });

  it("returns inspection above 0.8", () => {
    expect(zoomToLod(1.0)).toBe("inspection");
  });

  it("boundary 0.4 is navigation", () => {
    expect(zoomToLod(0.4)).toBe("navigation");
  });

  it("boundary 0.8 is inspection", () => {
    expect(zoomToLod(0.8)).toBe("inspection");
  });
});

describe("lodConfig", () => {
  it("overview hides all labels", () => {
    const config = lodConfig("overview");
    expect(config.showFileLabels).toBe(false);
    expect(config.showSymbolLabels).toBe(false);
    expect(config.showSignatures).toBe(false);
    expect(config.showEdgeLabels).toBe(false);
  });

  it("inspection shows signatures", () => {
    expect(lodConfig("inspection").showSignatures).toBe(true);
  });
});
