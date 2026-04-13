import type { ExplorerEdgeRelation, ExplorerNodeKind } from "../types";

export type RenderNode = {
  id: string;
  label: string;
  kind: ExplorerNodeKind;
  x: number;
  y: number;
  width: number;
  height: number;
  opacity: number;
  borderColor: number;
  borderWidth: number;
  bgColor: number;
  isFocused: boolean;
  isEgoUpstream: boolean;
  isEgoDownstream: boolean;
  signature?: {
    params: Array<{ name: string; typeAnnotation?: string }>;
    returnType?: string;
  };
};

export type RenderEdge = {
  id: string;
  fromId: string;
  toId: string;
  relation: ExplorerEdgeRelation;
  color: number;
  width: number;
  dashed: boolean;
  opacity: number;
  hasParticle?: boolean;
};

export type RenderGraph = {
  nodes: RenderNode[];
  edges: RenderEdge[];
};

export type LodLevel = "overview" | "navigation" | "inspection";
