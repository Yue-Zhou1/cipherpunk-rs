import * as PIXI from "pixi.js";

import type { LodConfig } from "./LodController";
import type { RenderNode } from "./types";

export class NodeLayer {
  private container: PIXI.Container;
  private graphics: PIXI.Graphics;
  private textCache = new Map<string, PIXI.Text>();
  private visibleNodes: RenderNode[] = [];

  constructor(stage: PIXI.Container) {
    this.container = new PIXI.Container();
    this.graphics = new PIXI.Graphics();
    this.container.addChild(this.graphics);
    stage.addChild(this.container);
  }

  draw(nodes: RenderNode[], lod: LodConfig): void {
    this.graphics.clear();

    const isCluster = (kind: string) => kind === "crate" || kind === "module";
    const visibleNodes = lod.showFileLabels ? nodes : nodes.filter((node) => isCluster(node.kind));
    this.visibleNodes = visibleNodes;

    const activeIds = new Set(visibleNodes.map((node) => node.id));
    for (const [id, text] of this.textCache) {
      if (!activeIds.has(id)) {
        this.container.removeChild(text);
        text.destroy();
        this.textCache.delete(id);
      }
    }

    for (const node of visibleNodes) {
      const { x, y, width, height, bgColor, borderColor, borderWidth, opacity } = node;
      const radius = node.kind === "crate" ? 8 : node.kind === "module" ? 6 : 4;

      this.graphics
        .roundRect(x, y, width, height, radius)
        .fill({ color: bgColor, alpha: opacity })
        .stroke({ color: borderColor, width: borderWidth, alpha: opacity });

      const showLabel =
        isCluster(node.kind) ||
        (node.kind === "file" && lod.showFileLabels) ||
        (!isCluster(node.kind) && node.kind !== "file" && lod.showSymbolLabels);

      if (showLabel) {
        let text = this.textCache.get(node.id);
        if (!text) {
          text = new PIXI.Text({
            text: node.label,
            style: new PIXI.TextStyle({
              fill: 0xf1f5f9,
              fontSize: node.kind === "crate" || node.kind === "module" ? 12 : 13,
              fontWeight: node.kind === "crate" || node.kind === "module" ? "600" : "500",
              fontFamily: "system-ui, sans-serif",
            }),
          });
          this.textCache.set(node.id, text);
          this.container.addChild(text);
        }

        text.text = node.label;
        text.x = x + 8;
        text.y = y + height / 2 - text.height / 2;
        text.alpha = opacity;
      } else {
        const text = this.textCache.get(node.id);
        if (text) {
          this.container.removeChild(text);
          text.destroy();
          this.textCache.delete(node.id);
        }
      }
    }
  }

  getVisibleIds(): Set<string> {
    return new Set(this.visibleNodes.map((node) => node.id));
  }

  hitTest(x: number, y: number): string | null {
    for (const node of [...this.visibleNodes].reverse()) {
      if (x >= node.x && x <= node.x + node.width && y >= node.y && y <= node.y + node.height) {
        return node.id;
      }
    }
    return null;
  }

  destroy(): void {
    for (const text of this.textCache.values()) {
      text.destroy();
    }
    this.textCache.clear();
    this.container.destroy({ children: true });
  }
}
