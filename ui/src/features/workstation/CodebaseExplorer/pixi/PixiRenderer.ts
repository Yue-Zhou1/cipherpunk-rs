import * as PIXI from "pixi.js";

export type PixiRendererOptions = {
  canvas: HTMLCanvasElement;
  onNodeClick: (nodeId: string) => void;
  onNodeRightClick: (nodeId: string, x: number, y: number) => void;
  onPaneClick: () => void;
};

export class PixiRenderer {
  private app: PIXI.Application;
  private options: PixiRendererOptions;

  constructor(options: PixiRendererOptions) {
    this.options = options;
    this.app = new PIXI.Application();
  }

  async init(): Promise<void> {
    await this.app.init({
      canvas: this.options.canvas,
      background: 0x0a0f1a,
      antialias: true,
      autoDensity: true,
      resolution: window.devicePixelRatio || 1,
      resizeTo: this.options.canvas.parentElement ?? this.options.canvas,
    });
  }

  resize(): void {
    this.app.resize();
  }

  destroy(): void {
    this.app.destroy(false, { children: true });
  }
}
