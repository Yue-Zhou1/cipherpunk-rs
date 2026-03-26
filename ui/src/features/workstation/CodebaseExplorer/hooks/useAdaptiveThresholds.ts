import { useCallback, useMemo, useState } from "react";

import type { GranularityLevel } from "../types";

type ResolvedGranularity = "files" | "modules" | "crates";

export function useAdaptiveThresholds(fileCount: number) {
  const [granularity, setGranularity] = useState<GranularityLevel>("auto");
  const [thresholds, setThresholds] = useState({ small: 30, large: 150 });

  const resolvedGranularity: ResolvedGranularity = useMemo(() => {
    if (granularity !== "auto") {
      return granularity;
    }
    if (fileCount < thresholds.small) {
      return "files";
    }
    if (fileCount > thresholds.large) {
      return "crates";
    }
    return "modules";
  }, [fileCount, granularity, thresholds]);

  const setThresholdsSafe = useCallback((next: { small: number; large: number }) => {
    const safeSmall = Math.max(1, Math.round(next.small));
    const safeLarge = Math.max(safeSmall + 1, Math.round(next.large));
    setThresholds({ small: safeSmall, large: safeLarge });
  }, []);

  return {
    granularity,
    setGranularity,
    resolvedGranularity,
    thresholds,
    setThresholds: setThresholdsSafe,
  };
}
