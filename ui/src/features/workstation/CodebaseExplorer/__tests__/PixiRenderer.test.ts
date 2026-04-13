import { beforeEach, describe, expect, it, vi } from "vitest";

// Pixi.js requires a real WebGL context in the browser, so mock it in jsdom.
vi.mock("pixi.js", () => {
  const stage = {
    addChild: vi.fn(),
    addChildAt: vi.fn(),
    x: 0,
    y: 0,
    scale: { x: 1, y: 1, set: vi.fn() },
  };
  const mockCanvas = {
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    setPointerCapture: vi.fn(),
    releasePointerCapture: vi.fn(),
    getBoundingClientRect: vi.fn(() => ({ left: 0, top: 0 })),
  } as unknown as HTMLCanvasElement;
  const mockApp = {
    init: vi.fn().mockResolvedValue(undefined),
    resize: vi.fn(),
    destroy: vi.fn(),
    stage,
    canvas: mockCanvas,
    ticker: { add: vi.fn() },
  };
  return {
    Application: vi.fn(() => mockApp),
    Container: vi.fn(() => ({
      addChild: vi.fn(),
      addChildAt: vi.fn(),
      removeChild: vi.fn(),
      destroy: vi.fn(),
    })),
    Graphics: vi.fn(() => ({
      clear: vi.fn().mockReturnThis(),
      roundRect: vi.fn().mockReturnThis(),
      fill: vi.fn().mockReturnThis(),
      stroke: vi.fn().mockReturnThis(),
      moveTo: vi.fn().mockReturnThis(),
      bezierCurveTo: vi.fn().mockReturnThis(),
      lineTo: vi.fn().mockReturnThis(),
      circle: vi.fn().mockReturnThis(),
      alpha: 1,
      destroy: vi.fn(),
    })),
    Text: vi.fn(() => ({ text: "", x: 0, y: 0, height: 16, alpha: 1, destroy: vi.fn() })),
    TextStyle: vi.fn(),
    __mockApp: mockApp,
  };
});

import * as PIXI from "pixi.js";
import { PixiRenderer } from "../pixi/PixiRenderer";

describe("PixiRenderer", () => {
  let canvas: HTMLCanvasElement;
  const noop = () => {};

  beforeEach(() => {
    canvas = document.createElement("canvas");
    vi.clearAllMocks();
  });

  it("calls app.init with canvas and background color on init()", async () => {
    const renderer = new PixiRenderer({
      canvas,
      onNodeClick: noop,
      onNodeRightClick: noop,
      onPaneClick: noop,
    });

    await renderer.init();

    const mockApp = (
      PIXI as unknown as { __mockApp: { init: ReturnType<typeof vi.fn> } }
    ).__mockApp;
    expect(mockApp.init).toHaveBeenCalledOnce();
    const initArgs = mockApp.init.mock.calls[0][0];
    expect(initArgs.canvas).toBe(canvas);
    expect(initArgs.background).toBe(0x0a0f1a);
  });

  it("calls app.destroy on destroy()", async () => {
    const renderer = new PixiRenderer({
      canvas,
      onNodeClick: noop,
      onNodeRightClick: noop,
      onPaneClick: noop,
    });

    await renderer.init();
    renderer.destroy();

    const mockApp = (
      PIXI as unknown as { __mockApp: { destroy: ReturnType<typeof vi.fn> } }
    ).__mockApp;
    expect(mockApp.destroy).toHaveBeenCalledOnce();
  });

  it("calls app.resize on resize()", async () => {
    const renderer = new PixiRenderer({
      canvas,
      onNodeClick: noop,
      onNodeRightClick: noop,
      onPaneClick: noop,
    });

    await renderer.init();
    renderer.resize();

    const mockApp = (
      PIXI as unknown as { __mockApp: { resize: ReturnType<typeof vi.fn> } }
    ).__mockApp;
    expect(mockApp.resize).toHaveBeenCalledOnce();
  });
});
