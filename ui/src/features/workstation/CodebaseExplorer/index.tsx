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

  return (
    <div className="explorer-toolbar" role="toolbar" aria-label="Explorer controls">
      <select
        value={ctx.granularity}
        onChange={(event) => ctx.setGranularity(event.target.value as GranularityLevel)}
        className="explorer-granularity-select"
        aria-label="View granularity"
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
        disabled={ctx.stateKind === "trace"}
      />
      {ctx.matchingNodeIds ? (
        <span className="explorer-match-count">{ctx.matchingNodeIds.size} matches</span>
      ) : null}

      {ctx.stateKind !== "overview" ? (
        <div className="explorer-depth-control" role="group" aria-label="Depth control">
          <button
            onClick={() => ctx.setDepth(ctx.depth - 1)}
            disabled={ctx.depth <= 1}
            type="button"
            aria-label="Decrease depth"
          >
            -
          </button>
          <span className="explorer-depth-value">{ctx.depth}</span>
          <button
            onClick={() => ctx.setDepth(ctx.depth + 1)}
            disabled={ctx.depth >= 10}
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
  const ctx = useExplorer();

  return (
    <section className="explorer-root" aria-label="Codebase Explorer">
      <ExplorerToolbar />
      <div className="explorer-body">
        <div className="explorer-canvas-container">
          <ExplorerCanvas />
          <TraceOverlay />
        </div>
        {ctx.stateKind !== "overview" ? (
          <div className="explorer-panel-container">
            <ContextPanel />
          </div>
        ) : null}
      </div>
    </section>
  );
}

export default function CodebaseExplorer({ sessionId, onNavigateToSource }: CodebaseExplorerProps) {
  void sessionId;

  return (
    <ExplorerProvider onNavigateToSource={onNavigateToSource}>
      <ExplorerLayout />
    </ExplorerProvider>
  );
}
