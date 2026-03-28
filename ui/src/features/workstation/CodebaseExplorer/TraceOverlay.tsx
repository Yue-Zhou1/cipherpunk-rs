import { useExplorer } from "./ExplorerContext";

export function TraceOverlay() {
  const { focusNode, nodeMap, stateKind, traceResult } = useExplorer();

  if (stateKind !== "trace" || !traceResult) {
    return null;
  }

  return (
    <div className="explorer-trace-breadcrumbs" aria-live="polite">
      <span className="explorer-trace-label">
        {traceResult.direction === "upstream" ? "Origin trace" : "Destination trace"}
        {traceResult.parameterName ? `: ${traceResult.parameterName}` : ""}
      </span>
      <div className="explorer-trace-path">
        {traceResult.path.map((nodeId, index) => (
          <span key={nodeId}>
            {index > 0 ? <span className="explorer-trace-arrow"> -&gt; </span> : null}
            <button className="explorer-trace-step" onClick={() => focusNode(nodeId)} type="button">
              {nodeMap.get(nodeId)?.label ?? nodeId}
            </button>
          </span>
        ))}
      </div>
    </div>
  );
}
