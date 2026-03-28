import { useCallback, useState } from "react";

export function useDepthControl(initial = 2) {
  const [depth, setDepthRaw] = useState(initial);

  const setDepth = useCallback((value: number) => {
    setDepthRaw(Math.max(1, Math.min(10, Math.round(value))));
  }, []);

  return { depth, setDepth };
}
