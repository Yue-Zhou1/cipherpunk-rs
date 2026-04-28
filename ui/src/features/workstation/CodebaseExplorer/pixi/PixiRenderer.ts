import * as PIXI from "pixi.js";

import { EdgeLayer } from "./EdgeLayer";
import { lodConfig, zoomToLod } from "./LodController";
import { NodeLayer } from "./NodeLayer";
import type { RenderEdge, RenderGraph, RenderNode } from "./types";

export type PixiRendererOptions = {
  canvas: HTMLCanvasElement;
  onNodeClick: (nodeId: string) => void;
  onNodeRightClick: (nodeId: string, x: number, y: number) => void;
  onPaneClick: () => void;
};

export class PixiRenderer {
  private app: PIXI.Application;
  private options: PixiRendererOptions;
  private nodeLayer!: NodeLayer;
  private edgeLayer!: EdgeLayer;
  private initPromise: Promise<void> | null = null;
  private isInitialized = false;
  private isDestroyed = false;
  private pendingParticleEdges: RenderEdge[] = [];
  private currentZoom = 1;
  private currentGraph: RenderGraph | null = null;
  private isPanning = false;
  private hasDragged = false;
  private panStart = { x: 0, y: 0 };
  private stageStartPos = { x: 0, y: 0 };
  private static readonly DRAG_THRESHOLD = 4;

  constructor(options: PixiRendererOptions) {
    this.options = options;
    this.app = new PIXI.Application();
  }

  async init(): Promise<void> {
    if (this.initPromise) {
      return this.initPromise;
    }

    this.initPromise = (async () => {
      await this.app.init({
        canvas: this.options.canvas,
        background: 0x0a0f1a,
        antialias: true,
        autoDensity: true,
        resolution: window.devicePixelRatio || 1,
        resizeTo: this.options.canvas.parentElement ?? this.options.canvas,
      });

      if (this.isDestroyed) {
        this.app.destroy(false, { children: true });
        return;
      }

      this.nodeLayer = new NodeLayer(this.app.stage);
      this.edgeLayer = new EdgeLayer(this.app.stage);
      this.isInitialized = true;

      this.app.canvas.addEventListener("click", this.handleClick);
      this.app.canvas.addEventListener("contextmenu", this.handleContextMenu);
      this.app.canvas.addEventListener("wheel", this.handleWheel, { passive: false });
      this.app.canvas.addEventListener("pointerdown", this.handlePointerDown);
      this.app.canvas.addEventListener("pointermove", this.handlePointerMove);
      this.app.canvas.addEventListener("pointerup", this.handlePointerUp);
      this.app.canvas.addEventListener("pointercancel", this.handlePointerUp);

      this.app.ticker.add((ticker) => {
        if (!this.currentGraph) {
          return;
        }
        const nodeById = new Map<string, RenderNode>(
          this.currentGraph.nodes.map((node) => [node.id, node])
        );
        this.edgeLayer.tickParticles(nodeById, ticker.deltaMS);
      });

      if (this.currentGraph) {
        this.redraw();
      }
      if (this.pendingParticleEdges.length > 0) {
        this.edgeLayer.setParticleEdges(this.pendingParticleEdges);
      }
    })();

    return this.initPromise;
  }

  resize(): void {
    if (!this.isInitialized || this.isDestroyed) {
      return;
    }
    this.app.resize();
  }

  updateGraph(graph: RenderGraph): void {
    if (this.isDestroyed) {
      return;
    }
    this.currentGraph = graph;
    if (this.isInitialized) {
      this.redraw();
    }
  }

  setParticleEdges(edges: RenderEdge[]): void {
    if (this.isDestroyed) {
      return;
    }
    this.pendingParticleEdges = edges;
    if (this.isInitialized) {
      this.edgeLayer.setParticleEdges(edges);
    }
  }

  private redraw(): void {
    if (!this.currentGraph || !this.isInitialized || this.isDestroyed) {
      return;
    }

    const lod = lodConfig(zoomToLod(this.currentZoom));
    const nodeById = new Map<string, RenderNode>(
      this.currentGraph.nodes.map((node) => [node.id, node])
    );

    this.nodeLayer.draw(this.currentGraph.nodes, lod);
    const visibleIds = this.nodeLayer.getVisibleIds();
    this.edgeLayer.draw(this.currentGraph.edges, nodeById, lod, visibleIds);
  }

  private handlePointerDown = (event: PointerEvent): void => {
    if (event.button !== 0) {
      return;
    }

    this.isPanning = true;
    this.hasDragged = false;
    this.panStart = { x: event.clientX, y: event.clientY };
    this.stageStartPos = { x: this.app.stage.x, y: this.app.stage.y };
    this.app.canvas.setPointerCapture(event.pointerId);
  };

  private handlePointerMove = (event: PointerEvent): void => {
    if (!this.isPanning) {
      return;
    }

    const dx = event.clientX - this.panStart.x;
    const dy = event.clientY - this.panStart.y;
    if (!this.hasDragged && Math.hypot(dx, dy) > PixiRenderer.DRAG_THRESHOLD) {
      this.hasDragged = true;
    }

    if (this.hasDragged) {
      this.app.stage.x = this.stageStartPos.x + dx;
      this.app.stage.y = this.stageStartPos.y + dy;
    }
  };

  private handlePointerUp = (event: PointerEvent): void => {
    if (!this.isPanning) {
      return;
    }
    this.isPanning = false;
    this.app.canvas.releasePointerCapture(event.pointerId);
  };

  private handleClick = (event: MouseEvent): void => {
    // Suppress click if this interaction ended as a pan.
    if (this.hasDragged) {
      return;
    }

    const { x, y } = this.canvasToWorld(event.offsetX, event.offsetY);
    const nodeId = this.nodeLayer.hitTest(x, y);
    if (nodeId) {
      this.options.onNodeClick(nodeId);
    } else {
      this.options.onPaneClick();
    }
  };

  private handleContextMenu = (event: MouseEvent): void => {
    event.preventDefault();
    const { x, y } = this.canvasToWorld(event.offsetX, event.offsetY);
    const nodeId = this.nodeLayer.hitTest(x, y);
    if (nodeId) {
      this.options.onNodeRightClick(nodeId, event.clientX, event.clientY);
    }
  };

  private handleWheel = (event: WheelEvent): void => {
    event.preventDefault();

    const scaleFactor = event.deltaY < 0 ? 1.1 : 0.9;
    const newScale = Math.max(0.1, Math.min(10, this.app.stage.scale.x * scaleFactor));

    const rect = this.app.canvas.getBoundingClientRect();
    const mx = event.clientX - rect.left;
    const my = event.clientY - rect.top;
    const worldX = (mx - this.app.stage.x) / this.app.stage.scale.x;
    const worldY = (my - this.app.stage.y) / this.app.stage.scale.y;

    this.app.stage.scale.set(newScale);
    this.app.stage.x = mx - worldX * newScale;
    this.app.stage.y = my - worldY * newScale;
    this.currentZoom = newScale;

    if (this.currentGraph) {
      this.redraw();
    }
  };

  private canvasToWorld(screenX: number, screenY: number): { x: number; y: number } {
    const scale = this.app.stage.scale.x;
    return {
      x: (screenX - this.app.stage.x) / scale,
      y: (screenY - this.app.stage.y) / scale,
    };
  }

  destroy(): void {
    if (this.isDestroyed) {
      return;
    }
    this.isDestroyed = true;

    if (!this.isInitialized) {
      return;
    }

    const canvas = this.app.canvas;
    canvas.removeEventListener("click", this.handleClick);
    canvas.removeEventListener("contextmenu", this.handleContextMenu);
    canvas.removeEventListener("wheel", this.handleWheel);
    canvas.removeEventListener("pointerdown", this.handlePointerDown);
    canvas.removeEventListener("pointermove", this.handlePointerMove);
    canvas.removeEventListener("pointerup", this.handlePointerUp);
    canvas.removeEventListener("pointercancel", this.handlePointerUp);
    this.nodeLayer.destroy();
    this.edgeLayer.destroy();
    this.app.destroy(false, { children: true });
    this.isInitialized = false;
  }
}
