import { ContextPanel } from "./ContextPanel";
import { ExplorerCanvas } from "./ExplorerCanvas";
import { ExplorerProvider, useExplorer } from "./ExplorerContext";
import { TraceOverlay } from "./TraceOverlay";
import type { GranularityLevel } from "./types";

type CodebaseExplorerProps = {
  sessionId: string;
  onNavigateToSource?: (filePath: string, line?: number) => void;
};

function ExplorerToolbar() {
  const ctx = useExplorer();
  const controlsDisabled = ctx.isLoading || !!ctx.error;

  return (
    <div className="explorer-toolbar" role="toolbar" aria-label="Explorer controls">
      <select
        value={ctx.granularity}
        onChange={(event) => ctx.setGranularity(event.target.value as GranularityLevel)}
        className="explorer-granularity-select"
        aria-label="View granularity"
        disabled={controlsDisabled}
      >
        <option value="auto">Auto</option>
        <option value="files">Files</option>
        <option value="modules">Modules</option>
        <option value="crates">Crates</option>
      </select>

      <input
        type="text"
        placeholder="Search nodes..."
        value={ctx.searchQuery}
        onChange={(event) => ctx.setSearchQuery(event.target.value)}
        className="explorer-search"
        aria-label="Search nodes"
        disabled={controlsDisabled || ctx.stateKind === "trace"}
      />
      {ctx.matchingNodeIds ? (
        <span className="explorer-match-count">{ctx.matchingNodeIds.size} matches</span>
      ) : null}

      {ctx.stateKind !== "overview" ? (
        <div className="explorer-depth-control" role="group" aria-label="Depth control">
          <button
            onClick={() => ctx.setDepth(ctx.depth - 1)}
            disabled={controlsDisabled || ctx.depth <= 1}
            type="button"
            aria-label="Decrease depth"
          >
            -
          </button>
          <span className="explorer-depth-value">{ctx.depth}</span>
          <button
            onClick={() => ctx.setDepth(ctx.depth + 1)}
            disabled={controlsDisabled || ctx.depth >= 10}
            type="button"
            aria-label="Increase depth"
          >
            +
          </button>
        </div>
      ) : null}

      <span className="explorer-state-badge">{ctx.stateKind.toUpperCase()}</span>
    </div>
  );
}

function ExplorerLayout() {
  const { isLoading, error, isStale, reload, stateKind } = useExplorer();

  return (
    <section className="explorer-root" aria-label="Codebase Explorer">
      {isStale ? (
        <div className="explorer-stale-banner" role="status">
          <span>Graph data has been updated.</span>
          <button type="button" onClick={reload}>
            Reload
          </button>
        </div>
      ) : null}
      <ExplorerToolbar />
      {isLoading ? (
        <div className="explorer-loading" role="status" aria-label="Loading graph">
          <div className="explorer-spinner" />
          <p>Loading project graph...</p>
        </div>
      ) : error ? (
        <div className="explorer-error" role="alert">
          <p>{error}</p>
          <button type="button" onClick={reload}>
            Retry
          </button>
        </div>
      ) : (
        <div className="explorer-body">
          <div className="explorer-canvas-container">
            <ExplorerCanvas />
            <TraceOverlay />
          </div>
          {stateKind !== "overview" ? (
            <div className="explorer-panel-container">
              <ContextPanel />
            </div>
          ) : null}
        </div>
      )}
    </section>
  );
}

export default function CodebaseExplorer({
  sessionId,
  onNavigateToSource,
}: CodebaseExplorerProps) {
  return (
    <ExplorerProvider sessionId={sessionId} onNavigateToSource={onNavigateToSource}>
      <ExplorerLayout />
    </ExplorerProvider>
  );
}
