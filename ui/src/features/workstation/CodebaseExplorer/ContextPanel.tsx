import { useMemo, useState } from "react";

import { useExplorer } from "./ExplorerContext";

export function ContextPanel() {
  const ctx = useExplorer();
  const [expandedSections, setExpandedSections] = useState<Set<string>>(new Set());

  const focusedNode = useMemo(
    () => (ctx.focusedNodeId ? ctx.nodeMap.get(ctx.focusedNodeId) ?? null : null),
    [ctx.focusedNodeId, ctx.nodeMap]
  );

  if (ctx.stateKind === "overview" || !focusedNode) {
    return null;
  }

  const callerCount = ctx.upstreamIds.size;
  const calleeCount = ctx.downstreamIds.size;

  const toggleSection = (name: string) => {
    setExpandedSections((previous) => {
      const next = new Set(previous);
      if (next.has(name)) {
        next.delete(name);
      } else {
        next.add(name);
      }
      return next;
    });
  };

  const callers = ctx.graph.edges
    .filter((edge) => edge.relation === "calls" && edge.to === focusedNode.id)
    .map((edge) => ctx.nodeMap.get(edge.from))
    .filter((node): node is NonNullable<typeof node> => Boolean(node));

  const callees = ctx.graph.edges
    .filter((edge) => edge.relation === "calls" && edge.from === focusedNode.id)
    .map((edge) => ctx.nodeMap.get(edge.to))
    .filter((node): node is NonNullable<typeof node> => Boolean(node));

  return (
    <aside className="explorer-context-panel" aria-label="Node context" aria-live="polite">
      <div className="explorer-ctx-header">
        <div className="explorer-ctx-name">{focusedNode.label}</div>
        <div className="explorer-ctx-location">
          {focusedNode.filePath ?? ""}
          {focusedNode.line ? `:${focusedNode.line}` : ""}
        </div>
      </div>

      {focusedNode.signature ? (
        <div className="explorer-ctx-signature">
          <span className="explorer-ctx-fn">fn </span>
          <span>{focusedNode.label}</span>
          <span>(</span>
          {focusedNode.signature.parameters.map((parameter, index) => (
            <span key={`${parameter.name}:${parameter.position}`}>
              {index > 0 ? ", " : ""}
              <button
                className="explorer-ctx-param"
                onClick={() => ctx.traceParameter(parameter.name)}
                type="button"
                title={`Trace origin of ${parameter.name}`}
              >
                {parameter.name}
                {parameter.typeAnnotation ? `: ${parameter.typeAnnotation}` : ""}
              </button>
            </span>
          ))}
          <span>)</span>
          {focusedNode.signature.returnType ? (
            <button
              className="explorer-ctx-return"
              onClick={ctx.traceReturn}
              type="button"
              title="Trace output destination"
            >
              {" -> "}
              {focusedNode.signature.returnType}
            </button>
          ) : null}
        </div>
      ) : null}

      <div className="explorer-ctx-counts">
        <span>
          {callerCount} caller{callerCount !== 1 ? "s" : ""}
        </span>
        <span> . </span>
        <span>
          {calleeCount} callee{calleeCount !== 1 ? "s" : ""}
        </span>
      </div>

      {ctx.traceResult ? (
        <div className="explorer-ctx-trace">
          <div className="explorer-ctx-trace-label">
            {ctx.traceResult.direction === "upstream" ? "Origin" : "Destination"} trace
            {ctx.traceResult.parameterName ? `: ${ctx.traceResult.parameterName}` : ""}
          </div>
          <div className="explorer-ctx-trace-path">
            {ctx.traceResult.path.map((id, index) => {
              const node = ctx.nodeMap.get(id);
              return (
                <span key={id}>
                  {index > 0 ? " -> " : ""}
                  <button
                    className="explorer-ctx-trace-step"
                    onClick={() => ctx.focusNode(id)}
                    type="button"
                  >
                    {node?.label ?? id.split("::").pop()}
                  </button>
                </span>
              );
            })}
          </div>
        </div>
      ) : null}

      {ctx.deadEndMessage ? <div className="explorer-ctx-deadend">{ctx.deadEndMessage}</div> : null}

      <div className="explorer-ctx-section">
        <button
          className="explorer-ctx-section-toggle"
          onClick={() => toggleSection("source")}
          type="button"
        >
          Source Code
          <span>{expandedSections.has("source") ? "▾" : "▸"}</span>
        </button>
        {expandedSections.has("source") ? (
          <div className="explorer-ctx-source-preview">
            <pre className="explorer-ctx-code">{`// Source loading deferred to Phase 2 API integration\n// File: ${focusedNode.filePath ?? "unknown"}${focusedNode.line ? `:${focusedNode.line}` : ""}`}</pre>
          </div>
        ) : null}
      </div>

      <div className="explorer-ctx-section">
        <button
          className="explorer-ctx-section-toggle"
          onClick={() => toggleSection("dataflow")}
          type="button"
        >
          Dataflow In/Out
          <span>{expandedSections.has("dataflow") ? "▾" : "▸"}</span>
        </button>
        {expandedSections.has("dataflow") ? (
          <div className="explorer-ctx-dataflow">
            {focusedNode.signature?.parameters.map((parameter) => {
              const inEdges = ctx.graph.edges.filter(
                (edge) =>
                  edge.relation === "parameter_flow" &&
                  edge.to === focusedNode.id &&
                  edge.parameterName === parameter.name
              );

              return (
                <div key={parameter.name} className="explorer-ctx-dataflow-row">
                  <span className="explorer-ctx-dataflow-param">{parameter.name}</span>
                  <span className="explorer-ctx-dataflow-arrow"> &lt;- </span>
                  {inEdges.length > 0
                    ? inEdges.map((edge) => {
                        const source = ctx.nodeMap.get(edge.from);
                        return (
                          <span key={edge.from} className="explorer-ctx-dataflow-src">
                            {source?.label ?? edge.from}
                          </span>
                        );
                      })
                    : <span className="explorer-ctx-dataflow-none">local/literal</span>}
                </div>
              );
            })}
          </div>
        ) : null}
      </div>

      <div className="explorer-ctx-section">
        <button
          className="explorer-ctx-section-toggle"
          onClick={() => toggleSection("fullpath")}
          type="button"
        >
          Full Call Path
          <span>{expandedSections.has("fullpath") ? "▾" : "▸"}</span>
        </button>
        {expandedSections.has("fullpath") ? (
          <div className="explorer-ctx-fullpath">
            <button
              className="explorer-ctx-trace-btn"
              onClick={() => ctx.traceParameter(focusedNode.signature?.parameters[0]?.name ?? "")}
              type="button"
            >
              Trace from entry points
            </button>
          </div>
        ) : null}
      </div>

      <div className="explorer-ctx-section">
        <button
          className="explorer-ctx-section-toggle"
          onClick={() => toggleSection("callers")}
          type="button"
        >
          Callers ({callers.length})
          <span>{expandedSections.has("callers") ? "▾" : "▸"}</span>
        </button>
        {expandedSections.has("callers") ? (
          <ul className="explorer-ctx-list">
            {callers.map((node) => (
              <li key={node.id}>
                <button onClick={() => ctx.focusNode(node.id)} type="button">
                  {node.label}
                  {node.filePath ? ` - ${node.filePath}` : ""}
                </button>
              </li>
            ))}
          </ul>
        ) : null}
      </div>

      <div className="explorer-ctx-section">
        <button
          className="explorer-ctx-section-toggle"
          onClick={() => toggleSection("callees")}
          type="button"
        >
          Callees ({callees.length})
          <span>{expandedSections.has("callees") ? "▾" : "▸"}</span>
        </button>
        {expandedSections.has("callees") ? (
          <ul className="explorer-ctx-list">
            {callees.map((node) => (
              <li key={node.id}>
                <button onClick={() => ctx.focusNode(node.id)} type="button">
                  {node.label}
                  {node.filePath ? ` - ${node.filePath}` : ""}
                </button>
              </li>
            ))}
          </ul>
        ) : null}
      </div>

      {focusedNode.filePath && ctx.onNavigateToSource ? (
        <button
          className="explorer-ctx-source-btn"
          onClick={() => ctx.onNavigateToSource?.(focusedNode.filePath!, focusedNode.line)}
          type="button"
        >
          Open in editor
        </button>
      ) : null}
    </aside>
  );
}
