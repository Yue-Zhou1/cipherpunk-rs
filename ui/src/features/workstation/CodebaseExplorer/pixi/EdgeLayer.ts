import * as PIXI from "pixi.js";

import type { LodConfig } from "./LodController";
import type { RenderEdge, RenderNode } from "./types";

export class EdgeLayer {
  private graphics: PIXI.Graphics;
  private particleGraphics: PIXI.Graphics;
  private particles: Array<{
    edgeId: string;
    t: number;
    color: number;
    fromId: string;
    toId: string;
  }> = [];

  constructor(stage: PIXI.Container) {
    this.graphics = new PIXI.Graphics();
    this.particleGraphics = new PIXI.Graphics();
    stage.addChildAt(this.graphics, 0);
    stage.addChild(this.particleGraphics);
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

  setParticleEdges(edges: RenderEdge[]): void {
    const edgeById = new Map(edges.map((edge) => [edge.id, edge]));
    this.particles = this.particles.filter((particle) => {
      const edge = edgeById.get(particle.edgeId);
      if (!edge) {
        return false;
      }
      particle.color = edge.color;
      particle.fromId = edge.fromId;
      particle.toId = edge.toId;
      return true;
    });

    for (const edge of edges) {
      const existingCount = this.particles.filter((particle) => particle.edgeId === edge.id).length;
      for (let index = existingCount; index < 2; index += 1) {
        this.particles.push({
          edgeId: edge.id,
          t: index * 0.5,
          color: edge.color,
          fromId: edge.fromId,
          toId: edge.toId,
        });
      }
    }
  }

  tickParticles(nodeById: Map<string, RenderNode>, dt: number): void {
    const speed = 0.0008;
    this.particleGraphics.clear();

    for (const particle of this.particles) {
      particle.t = (particle.t + speed * dt) % 1;
      const from = nodeById.get(particle.fromId);
      const to = nodeById.get(particle.toId);
      if (!from || !to) {
        continue;
      }

      const x1 = from.x + from.width / 2;
      const y1 = from.y + from.height;
      const x2 = to.x + to.width / 2;
      const y2 = to.y;
      const midY = (y1 + y2) / 2;
      const t = particle.t;
      const mt = 1 - t;
      const px =
        mt * mt * mt * x1 + 3 * mt * mt * t * x1 + 3 * mt * t * t * x2 + t * t * t * x2;
      const py =
        mt * mt * mt * y1 + 3 * mt * mt * t * midY + 3 * mt * t * t * midY + t * t * t * y2;

      this.particleGraphics.circle(px, py, 3).fill({ color: particle.color });
    }
  }

  destroy(): void {
    this.particleGraphics.destroy();
    this.graphics.destroy();
  }
}
