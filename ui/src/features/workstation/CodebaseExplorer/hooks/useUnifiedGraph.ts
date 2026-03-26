import { useMemo } from "react";

import { largeFixture, mediumFixture, smallFixture } from "../fixtures/mockGraph";
import type { ExplorerGraph } from "../types";

type DatasetSize = "small" | "medium" | "large";

export function useUnifiedGraph(size: DatasetSize = "medium"): { graph: ExplorerGraph } {
  const graph = useMemo(() => {
    switch (size) {
      case "small":
        return smallFixture;
      case "large":
        return largeFixture;
      default:
        return mediumFixture;
    }
  }, [size]);

  return { graph };
}
