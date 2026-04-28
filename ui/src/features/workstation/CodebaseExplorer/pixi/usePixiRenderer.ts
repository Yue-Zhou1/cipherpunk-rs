import { useEffect, useRef, useState, type RefObject } from "react";

import { PixiRenderer, type PixiRendererOptions } from "./PixiRenderer";

type UsePixiRendererResult = {
  rendererRef: RefObject<PixiRenderer | null>;
  initError: string | null;
};

export function usePixiRenderer(
  canvasRef: RefObject<HTMLCanvasElement | null>,
  options: Omit<PixiRendererOptions, "canvas">
): UsePixiRendererResult {
  const rendererRef = useRef<PixiRenderer | null>(null);
  const [initError, setInitError] = useState<string | null>(null);

  // Keep stable proxy callbacks so renderer lifecycle does not depend on callback identity.
  const callbacksRef = useRef(options);
  callbacksRef.current = options;

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) {
      return;
    }
    let cancelled = false;

    const renderer = new PixiRenderer({
      canvas,
      onNodeClick: (nodeId) => callbacksRef.current.onNodeClick(nodeId),
      onNodeRightClick: (nodeId, x, y) =>
        callbacksRef.current.onNodeRightClick(nodeId, x, y),
      onPaneClick: () => callbacksRef.current.onPaneClick(),
    });
    rendererRef.current = renderer;
    setInitError(null);

    void Promise.resolve(renderer.init()).catch((error: unknown) => {
      if (cancelled) {
        return;
      }
      const reason = error instanceof Error ? error.message : String(error);
      setInitError(`Failed to initialize WebGL renderer: ${reason}`);
      rendererRef.current = null;
      console.error("Failed to initialize Pixi renderer", error);
    });

    const handleResize = () => renderer.resize();
    window.addEventListener("resize", handleResize);

    return () => {
      cancelled = true;
      window.removeEventListener("resize", handleResize);
      renderer.destroy();
      rendererRef.current = null;
    };
  }, [canvasRef]); // canvasRef object identity is stable; this effect is intentionally mount/unmount scoped.

  return { rendererRef, initError };
}
