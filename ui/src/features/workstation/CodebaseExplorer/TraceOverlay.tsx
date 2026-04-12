import { useExplorer } from "./ExplorerContext";

export function TraceOverlay() {
  const { clearHighlight, neighborhoodResult } = useExplorer();

  if (!neighborhoodResult) {
    return null;
  }

  const highlightedCount = Math.max(0, neighborhoodResult.highlightedIds.size - 1);

  return (
    <div className="explorer-trace-breadcrumbs" aria-live="polite">
      <span className="explorer-trace-label">
        {`Showing ${highlightedCount} ${
          neighborhoodResult.direction === "upstream" ? "callers" : "callees"
        }`}
      </span>
      <button className="explorer-trace-step" onClick={clearHighlight} type="button">
        Clear
      </button>
    </div>
  );
}
