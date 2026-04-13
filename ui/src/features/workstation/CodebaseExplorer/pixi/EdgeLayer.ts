import * as PIXI from "pixi.js";

import type { LodConfig } from "./LodController";
import type { RenderEdge, RenderNode } from "./types";

export class EdgeLayer {
  private graphics: PIXI.Graphics;

  constructor(stage: PIXI.Container) {
    this.graphics = new PIXI.Graphics();
    stage.addChildAt(this.graphics, 0);
  }

  draw(
    edges: RenderEdge[],
    nodeById: Map<string, RenderNode>,
    lod: LodConfig,
    visibleIds: Set<string>
  ): void {
    this.graphics.clear();

    for (const edge of edges) {
      const from = nodeById.get(edge.fromId);
      const to = nodeById.get(edge.toId);
      if (!from || !to || !visibleIds.has(edge.fromId) || !visibleIds.has(edge.toId)) {
        continue;
      }

      const drawWidth = lod.edgeWidth === 1 ? 1 : edge.width;

      const x1 = from.x + from.width / 2;
      const y1 = from.y + from.height;
      const x2 = to.x + to.width / 2;
      const y2 = to.y;
      const midY = (y1 + y2) / 2;

      if (edge.dashed) {
        const steps = 60;
        const dash = 8;
        const gap = 5;
        let accumulated = 0;
        let drawing = true;
        let prevX = x1;
        let prevY = y1;

        for (let i = 1; i <= steps; i += 1) {
          const t = i / steps;
          const mt = 1 - t;
          const cx =
            mt * mt * mt * x1 + 3 * mt * mt * t * x1 + 3 * mt * t * t * x2 + t * t * t * x2;
          const cy =
            mt * mt * mt * y1 + 3 * mt * mt * t * midY + 3 * mt * t * t * midY + t * t * t * y2;
          const segmentLength = Math.hypot(cx - prevX, cy - prevY);
          accumulated += segmentLength;

          const threshold = drawing ? dash : gap;
          if (accumulated >= threshold) {
            if (drawing) {
              this.graphics
                .moveTo(prevX, prevY)
                .lineTo(cx, cy)
                .stroke({ color: edge.color, width: drawWidth, alpha: edge.opacity });
            }
            drawing = !drawing;
            accumulated -= threshold;
          }

          prevX = cx;
          prevY = cy;
        }
      } else {
        this.graphics
          .moveTo(x1, y1)
          .bezierCurveTo(x1, midY, x2, midY, x2, y2)
          .stroke({ color: edge.color, width: drawWidth, alpha: edge.opacity });
      }

      const angle = Math.atan2(y2 - midY, x2 - x1);
      const arrowSize = 6;
      this.graphics
        .moveTo(x2, y2)
        .lineTo(
          x2 - arrowSize * Math.cos(angle - Math.PI / 6),
          y2 - arrowSize * Math.sin(angle - Math.PI / 6)
        )
        .lineTo(
          x2 - arrowSize * Math.cos(angle + Math.PI / 6),
          y2 - arrowSize * Math.sin(angle + Math.PI / 6)
        )
        .fill({ color: edge.color, alpha: edge.opacity });
    }
  }

  destroy(): void {
    this.graphics.destroy();
  }
}
