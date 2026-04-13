import type { LodLevel } from "./types";

export function zoomToLod(zoom: number): LodLevel {
  if (zoom < 0.4) {
    return "overview";
  }
  if (zoom < 0.8) {
    return "navigation";
  }
  return "inspection";
}

export type LodConfig = {
  showFileLabels: boolean;
  showSymbolLabels: boolean;
  showSignatures: boolean;
  showEdgeLabels: boolean;
  edgeWidth: number;
};

export function lodConfig(lod: LodLevel): LodConfig {
  if (lod === "overview") {
    return {
      showFileLabels: false,
      showSymbolLabels: false,
      showSignatures: false,
      showEdgeLabels: false,
      edgeWidth: 1,
    };
  }

  if (lod === "navigation") {
    return {
      showFileLabels: true,
      showSymbolLabels: true,
      showSignatures: false,
      showEdgeLabels: false,
      edgeWidth: 1.5,
    };
  }

  return {
    showFileLabels: true,
    showSymbolLabels: true,
    showSignatures: true,
    showEdgeLabels: true,
    edgeWidth: 1.5,
  };
}
