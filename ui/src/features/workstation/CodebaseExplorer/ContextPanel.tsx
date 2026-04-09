import { useEffect, useMemo, useRef, useState } from "react";

import { readSourceFile } from "../../../ipc/commands";
import { useExplorer } from "./ExplorerContext";

export function ContextPanel() {
  const ctx = useExplorer();
  const [expandedSections, setExpandedSections] = useState<Set<string>>(new Set());
  const [sourceSnippet, setSourceSnippet] = useState<string | null>(null);
  const [sourceLoading, setSourceLoading] = useState(false);
  const requestIdRef = useRef(0);

  const focusedNode = useMemo(
    () => (ctx.focusedNodeId ? ctx.nodeMap.get(ctx.focusedNodeId) ?? null : null),
    [ctx.focusedNodeId, ctx.nodeMap]
  );

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

  const callers = focusedNode
    ? ctx.graph.edges
        .filter((edge) => edge.relation === "calls" && edge.to === focusedNode.id)
        .map((edge) => ctx.nodeMap.get(edge.from))
        .filter((node): node is NonNullable<typeof node> => Boolean(node))
    : [];

  const callees = focusedNode
    ? ctx.graph.edges
        .filter((edge) => edge.relation === "calls" && edge.from === focusedNode.id)
        .map((edge) => ctx.nodeMap.get(edge.to))
        .filter((node): node is NonNullable<typeof node> => Boolean(node))
    : [];

  const highlightedNeighborCount = Math.max(
    0,
    (ctx.neighborhoodResult?.highlightedIds.size ?? 0) - 1
  );

  useEffect(() => {
    setSourceSnippet(null);
  }, [focusedNode?.id]);

  useEffect(() => {
    if (!expandedSections.has("source") || !focusedNode?.filePath) {
      return;
    }
    const requestId = ++requestIdRef.current;
    setSourceLoading(true);

    void readSourceFile(ctx.sessionId, focusedNode.filePath)
      .then((response) => {
        if (requestId !== requestIdRef.current) {
          return;
        }
        if (focusedNode.line) {
          const lines = response.content.split("\n");
          const start = Math.max(0, focusedNode.line - 8);
          const end = Math.min(lines.length, focusedNode.line + 7);
          setSourceSnippet(
            lines
              .slice(start, end)
              .map((line, index) => `${start + index + 1} | ${line}`)
              .join("\n")
          );
          return;
        }
        setSourceSnippet(
          response.content
            .split("\n")
            .slice(0, 30)
            .map((line, index) => `${index + 1} | ${line}`)
            .join("\n")
        );
      })
      .catch(() => {
        if (requestId !== requestIdRef.current) {
          return;
        }
        setSourceSnippet("// Failed to load source");
      })
      .finally(() => {
        if (requestId !== requestIdRef.current) {
          return;
        }
        setSourceLoading(false);
      });
  }, [ctx.sessionId, expandedSections, focusedNode?.filePath, focusedNode?.id, focusedNode?.line]);

  if (ctx.stateKind === "overview" || !focusedNode) {
    return null;
  }

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
              <span className="explorer-ctx-param">
                {parameter.name}
                {parameter.typeAnnotation ? `: ${parameter.typeAnnotation}` : ""}
              </span>
            </span>
          ))}
          <span>)</span>
          {focusedNode.signature.returnType ? (
            <span className="explorer-ctx-return">
              {" -> "}
              {focusedNode.signature.returnType}
            </span>
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

      <div className="explorer-ctx-actions">
        <button className="explorer-ctx-trace-btn" onClick={ctx.showCallers} type="button">
          Show callers
        </button>
        <button className="explorer-ctx-trace-btn" onClick={ctx.showCallees} type="button">
          Show callees
        </button>
      </div>

      {ctx.neighborhoodResult ? (
        <div className="explorer-ctx-highlight-summary">
          <span>
            Highlighting {highlightedNeighborCount}{" "}
            {ctx.neighborhoodResult.direction === "upstream" ? "callers" : "callees"}
          </span>
          <button type="button" onClick={ctx.clearHighlight}>
            Clear
          </button>
        </div>
      ) : null}

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
            {sourceLoading ? (
              <p className="explorer-ctx-source-loading">Loading...</p>
            ) : (
              <pre className="explorer-ctx-code">{sourceSnippet ?? "// No source available"}</pre>
            )}
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
          onClick={() => ctx.onNavigateToSource?.(focusedNode.filePath, focusedNode.line)}
          type="button"
        >
          Open in editor
        </button>
      ) : null}
    </aside>
  );
}
