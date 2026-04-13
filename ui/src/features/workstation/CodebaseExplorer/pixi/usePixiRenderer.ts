import { useEffect, useRef, type RefObject } from "react";

import { PixiRenderer, type PixiRendererOptions } from "./PixiRenderer";

export function usePixiRenderer(
  canvasRef: RefObject<HTMLCanvasElement | null>,
  options: Omit<PixiRendererOptions, "canvas">
): RefObject<PixiRenderer | null> {
  const rendererRef = useRef<PixiRenderer | null>(null);

  // Keep stable proxy callbacks so renderer lifecycle does not depend on callback identity.
  const callbacksRef = useRef(options);
  callbacksRef.current = options;

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) {
      return;
    }

    const renderer = new PixiRenderer({
      canvas,
      onNodeClick: (nodeId) => callbacksRef.current.onNodeClick(nodeId),
      onNodeRightClick: (nodeId, x, y) =>
        callbacksRef.current.onNodeRightClick(nodeId, x, y),
      onPaneClick: () => callbacksRef.current.onPaneClick(),
    });
    rendererRef.current = renderer;

    void renderer.init().catch((error: unknown) => {
      // Surface WebGL/context setup failures until we add UI-level error plumbing.
      console.error("Failed to initialize Pixi renderer", error);
    });

    const handleResize = () => renderer.resize();
    window.addEventListener("resize", handleResize);

    return () => {
      window.removeEventListener("resize", handleResize);
      renderer.destroy();
      rendererRef.current = null;
    };
  }, [canvasRef]); // canvasRef object identity is stable; this effect is intentionally mount/unmount scoped.

  return rendererRef;
}
