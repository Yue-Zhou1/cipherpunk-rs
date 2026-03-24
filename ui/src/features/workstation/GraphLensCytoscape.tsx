import { Suspense, lazy, useEffect, useMemo, useRef, useState } from "react";
import type { Core, ElementDefinition } from "cytoscape";

import {
  loadDataflowGraph,
  loadFeatureGraph,
  loadFileGraph,
  loadSymbolGraph,
  type GraphLensKind,
  type ProjectGraphResponse,
} from "../../ipc/commands";

export type GraphLensProps = {
  sessionId: string;
  selectedNodeIds?: string[];
  onNavigateToSource?: (filePath: string, line?: number) => void;
  focusSymbolName?: string | null;
};

const LENS_OPTIONS: Array<{ kind: GraphLensKind; label: string }> = [
  { kind: "file", label: "File Graph" },
  { kind: "feature", label: "Feature Graph" },
  { kind: "dataflow", label: "Dataflow Graph" },
  { kind: "symbol", label: "Symbol Graph" },
];
const CytoscapeComponent = lazy(() => import("react-cytoscapejs"));

function GraphPlaceholder({
  title,
  detail,
}: {
  title: string;
  detail: string;
}): JSX.Element {
  return (
    <div className="flex items-center justify-center h-full text-gray-500">
      <div className="text-center">
        <p className="text-lg font-medium">{title}</p>
        <p className="text-sm mt-1">{detail}</p>
      </div>
    </div>
  );
}

function GraphLens({
  sessionId,
  selectedNodeIds = [],
  onNavigateToSource,
  focusSymbolName,
}: GraphLensProps): JSX.Element {
  const [lens, setLens] = useState<GraphLensKind>("file");
  const [includeValues, setIncludeValues] = useState(false);
  const [graph, setGraph] = useState<ProjectGraphResponse | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState("");
  const [canRenderCytoscape, setCanRenderCytoscape] = useState(false);
  const cyRef = useRef<Core | null>(null);
  const containerRef = useRef<HTMLDivElement | null>(null);
  const initRafRef = useRef<number | null>(null);

  useEffect(() => {
    let cancelled = false;
    setIsLoading(true);
    setError(null);

    const request =
      lens === "file"
        ? loadFileGraph(sessionId)
        : lens === "feature"
          ? loadFeatureGraph(sessionId)
          : lens === "dataflow"
            ? loadDataflowGraph(sessionId, includeValues)
            : loadSymbolGraph(sessionId);

    void request
      .then((response) => {
        if (!cancelled) {
          setGraph(response);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setError("Unable to load graph lens.");
          setGraph(null);
        }
      })
      .finally(() => {
        if (!cancelled) {
          setIsLoading(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [includeValues, lens, sessionId]);

  useEffect(() => {
    if (lens !== "dataflow" && includeValues) {
      setIncludeValues(false);
    }
  }, [includeValues, lens]);

  const title = useMemo(
    () => LENS_OPTIONS.find((entry) => entry.kind === lens)?.label ?? "Graph Lens",
    [lens]
  );
  const isJsdom =
    typeof navigator !== "undefined" &&
    navigator.userAgent.toLowerCase().includes("jsdom");
  const selectedNodeIdSet = useMemo(() => new Set(selectedNodeIds), [selectedNodeIds]);
  const normalizedFocusSymbol = (focusSymbolName ?? "").trim().toLowerCase();
  const matchingNodeIds = useMemo(() => {
    if (!graph || !searchQuery.trim()) {
      return null;
    }
    const query = searchQuery.trim().toLowerCase();
    return new Set(
      graph.nodes
        .filter(
          (node) =>
            node.label.toLowerCase().includes(query) || node.id.toLowerCase().includes(query)
        )
        .map((node) => node.id)
    );
  }, [graph, searchQuery]);
  const elements = useMemo<ElementDefinition[]>(
    () =>
      graph
        ? [
            ...graph.nodes.map((node) => ({
              data: {
                id: node.id,
                label: node.label,
                kind: node.kind,
                filePath: node.filePath,
                line: node.line,
              },
              classes: [
                selectedNodeIdSet.has(node.id) ? "selected" : "",
                normalizedFocusSymbol.length > 0 &&
                (node.label.toLowerCase().includes(normalizedFocusSymbol) ||
                  node.id.toLowerCase().includes(`::${normalizedFocusSymbol}`))
                  ? "focus-match"
                  : "",
                matchingNodeIds && !matchingNodeIds.has(node.id) ? "search-dimmed" : "",
                node.maxSeverity ? `severity-${node.maxSeverity.toLowerCase()}` : "",
                node.findingCount && node.findingCount > 0 ? "has-findings" : "",
              ]
                .filter((value) => value.length > 0)
                .join(" "),
            })),
            ...graph.edges.map((edge, index) => ({
              data: {
                id: `${edge.from}-${edge.to}-${index}`,
                source: edge.from,
                target: edge.to,
                label: edge.valuePreview
                  ? `${edge.relation} (${edge.valuePreview})`
                  : edge.relation,
              },
            })),
          ]
        : [],
    [graph, matchingNodeIds, normalizedFocusSymbol, selectedNodeIdSet]
  );

  useEffect(() => {
    if (selectedNodeIds.length > 0 || normalizedFocusSymbol.length === 0 || !cyRef.current) {
      return;
    }
    const focused = cyRef.current
      .nodes()
      .filter((node) => node.hasClass("focus-match"));
    if (focused.length > 0) {
      cyRef.current.fit(focused, 36);
    }
  }, [normalizedFocusSymbol, selectedNodeIds.length]);

  useEffect(() => {
    if (isJsdom || isLoading || error || !graph) {
      setCanRenderCytoscape(false);
      return;
    }

    let cancelled = false;
    let retries = 0;
    const maxRetries = 5;
    const attemptInit = (): void => {
      if (cancelled) {
        return;
      }

      const container = containerRef.current;
      if (!container) {
        return;
      }

      if (container.offsetWidth > 0) {
        setCanRenderCytoscape(true);
        return;
      }

      if (retries >= maxRetries) {
        setCanRenderCytoscape(true);
        return;
      }

      retries += 1;
      initRafRef.current = requestAnimationFrame(attemptInit);
    };

    setCanRenderCytoscape(false);
    attemptInit();

    return () => {
      cancelled = true;
      if (initRafRef.current !== null) {
        cancelAnimationFrame(initRafRef.current);
        initRafRef.current = null;
      }
    };
  }, [error, graph, isJsdom, isLoading]);

  useEffect(() => {
    if (isJsdom || typeof ResizeObserver === "undefined") {
      return;
    }

    const container = containerRef.current;
    if (!container) {
      return;
    }

    const observer = new ResizeObserver((entries) => {
      for (const entry of entries) {
        if (entry.contentRect.width <= 0 || entry.contentRect.height <= 0) {
          continue;
        }
        cyRef.current?.resize();
        cyRef.current?.fit();
      }
    });
    observer.observe(container);

    return () => {
      observer.disconnect();
    };
  }, [graph, isJsdom, lens]);

  const fitToScreen = (): void => {
    if (!cyRef.current) {
      return;
    }
    if (matchingNodeIds && matchingNodeIds.size > 0) {
      const matching = cyRef.current
        .nodes()
        .filter((node) => matchingNodeIds.has(node.id()));
      if (matching.length > 0) {
        cyRef.current.fit(matching, 36);
        return;
      }
    }
    cyRef.current.fit();
  };

  return (
    <section className="panel workstation-graph-lens" aria-label="Graph Lens">
      <div className="workstation-panel-head">
        <p className="workstation-panel-eyebrow">Graph Lens</p>
        <h2>{title}</h2>
      </div>

      <div className="graph-lens-toolbar" role="tablist" aria-label="Graph lens selector">
        <select
          value={lens}
          onChange={(event) => setLens(event.target.value as GraphLensKind)}
          className="graph-lens-select"
          aria-label="Select graph lens"
        >
          {LENS_OPTIONS.map((entry) => (
            <option key={entry.kind} value={entry.kind}>
              {entry.label}
            </option>
          ))}
        </select>
        <input
          type="text"
          placeholder="Search..."
          value={searchQuery}
          onChange={(event) => setSearchQuery(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              fitToScreen();
            }
          }}
          className="graph-lens-search"
        />
        {lens === "dataflow" ? (
          <label className="graph-lens-toggle">
            <input
              type="checkbox"
              checked={includeValues}
              onChange={(event) => setIncludeValues(event.target.checked)}
            />
            <span>Values</span>
          </label>
        ) : null}
        {matchingNodeIds ? (
          <span className="muted-text">
            {matchingNodeIds.size} matches
          </span>
        ) : null}
        <button type="button" className="graph-lens-fit-button" onClick={fitToScreen}>
          Fit
        </button>
      </div>

      {isLoading ? <p className="muted-text">Loading graph...</p> : null}
      {selectedNodeIds.length > 0 ? (
        <p className="muted-text">Review context selected {selectedNodeIds.length} node(s).</p>
      ) : null}

      {!isLoading && error && !graph ? (
        <GraphPlaceholder
          title="No graph data available"
          detail="Run the BuildProjectIr job to generate the code graph."
        />
      ) : null}

      {!isLoading && !error && graph && graph.nodes.length === 0 ? (
        <GraphPlaceholder
          title="Graph is empty"
          detail="Graph is empty - no source files found in the selected scope."
        />
      ) : null}

      {!isLoading && !error && graph && graph.nodes.length > 0 ? (
        <>
          <p className="muted-text">
            {graph.nodes.length} nodes / {graph.edges.length} edges
          </p>
          {!isJsdom ? (
            <div
              ref={containerRef}
              className="graph-lens-canvas"
              style={{ minHeight: "300px", minWidth: "200px" }}
            >
              {canRenderCytoscape ? (
                <Suspense fallback={<p className="muted-text">Loading graph canvas...</p>}>
                  <CytoscapeComponent
                    elements={elements}
                    layout={{
                      name: lens === "dataflow" ? "breadthfirst" : "cose",
                      animate: false,
                      directed: true,
                      padding: 18,
                    }}
                    stylesheet={[
                      {
                        selector: "node",
                        style: {
                          label: "data(label)",
                          color: "#d4d4d4",
                          "font-size": "10px",
                          "text-wrap": "wrap",
                          "text-max-width": "160px",
                          "background-color":
                            lens === "dataflow" ? "#007acc" : "#0e639c",
                          width: "label",
                          padding: "8px",
                          shape: "round-rectangle",
                          "border-width": 1,
                          "border-color": "#2b2b2b",
                        },
                      },
                      {
                        selector: "node[filePath]",
                        style: {
                          cursor: "pointer",
                        },
                      },
                      {
                        selector: "node.search-dimmed",
                        style: {
                          opacity: 0.15,
                        },
                      },
                      {
                        selector: ".severity-critical",
                        style: { "border-color": "#dc2626", "border-width": 3 },
                      },
                      {
                        selector: ".severity-high",
                        style: { "border-color": "#ea580c", "border-width": 3 },
                      },
                      {
                        selector: ".severity-medium",
                        style: { "border-color": "#ca8a04", "border-width": 2 },
                      },
                      {
                        selector: ".severity-low",
                        style: { "border-color": "#2563eb", "border-width": 2 },
                      },
                      {
                        selector: ".has-findings",
                        style: { "background-opacity": 0.15 },
                      },
                      {
                        selector: "edge",
                        style: {
                          label: "data(label)",
                          color: "#9f9f9f",
                          "font-size": "9px",
                          "curve-style": "bezier",
                          width: 1.2,
                          "line-color": "#6b6b6b",
                          "target-arrow-color": "#6b6b6b",
                          "target-arrow-shape": "triangle",
                          "text-background-color": "#1f1f1f",
                          "text-background-opacity": 0.8,
                          "text-background-padding": "1px",
                        },
                      },
                      {
                        selector: "node.selected",
                        style: {
                          "border-color": "#ffd166",
                          "border-width": 2,
                          "background-color": "#0a84ff",
                        },
                      },
                      {
                        selector: "node.focus-match",
                        style: {
                          "border-color": "#8fd694",
                          "border-width": 2,
                        },
                      },
                    ]}
                    style={{ width: "100%", height: "100%" }}
                    minZoom={0.2}
                    maxZoom={2.4}
                    cy={(cy) => {
                      cyRef.current = cy;
                      cy.off("tap", "node");
                      cy.on("tap", "node", (event) => {
                        const filePath = event.target.data("filePath");
                        const line = event.target.data("line");
                        if (
                          onNavigateToSource &&
                          typeof filePath === "string" &&
                          filePath.length > 0
                        ) {
                          onNavigateToSource(
                            filePath,
                            typeof line === "number" ? line : undefined
                          );
                        }
                      });

                      // This runs whenever the Cytoscape instance is (re)created, so a selected
                      // review context will refit on graph reload/lens switch as well as selection.
                      if (selectedNodeIds.length === 0) {
                        if (normalizedFocusSymbol.length === 0) {
                          return;
                        }
                        const focused = cy
                          .nodes()
                          .filter((node) => node.hasClass("focus-match"));
                        if (focused.length > 0) {
                          cy.fit(focused, 36);
                        }
                        return;
                      }
                      const targets = cy
                        .nodes()
                        .filter((node) => selectedNodeIdSet.has(node.id()));
                      if (targets.length > 0) {
                        cy.fit(targets, 36);
                      }
                    }}
                  />
                </Suspense>
              ) : (
                <p className="muted-text">Preparing graph canvas...</p>
              )}
            </div>
          ) : (
            <div className="graph-lens-grid">
              <div className="graph-lens-block">
                <h3>Nodes</h3>
                <ul>
                  {graph.nodes.slice(0, 32).map((node) => (
                    <li
                      key={node.id}
                      data-testid="graph-node-row"
                      className={[
                        selectedNodeIdSet.has(node.id) ? "selected" : "",
                        matchingNodeIds && !matchingNodeIds.has(node.id) ? "search-dimmed" : "",
                        node.maxSeverity ? `severity-${node.maxSeverity.toLowerCase()}` : "",
                      ]
                        .filter(Boolean)
                        .join(" ") || undefined}
                    >
                      {node.filePath && onNavigateToSource ? (
                        <button
                          type="button"
                          className="graph-node-link"
                          onClick={() => onNavigateToSource(node.filePath!, node.line)}
                        >
                          <code>{node.label}</code>
                        </button>
                      ) : (
                        <code>{node.label}</code>
                      )}
                      {node.findingCount && node.findingCount > 0 ? (
                        <span
                          data-testid="graph-node-finding-badge"
                          className={`graph-node-finding-badge severity-${(node.maxSeverity ?? "low").toLowerCase()}`}
                        >
                          {node.findingCount}
                        </span>
                      ) : null}
                    </li>
                  ))}
                </ul>
              </div>
              <div className="graph-lens-block">
                <h3>Edges</h3>
                <ul>
                  {graph.edges.slice(0, 8).map((edge, index) => (
                    <li key={`${edge.from}-${edge.to}-${index}`}>
                      <code>
                        {edge.relation}: {edge.from} -&gt; {edge.to}
                      </code>
                      {edge.valuePreview ? <span> ({edge.valuePreview})</span> : null}
                    </li>
                  ))}
                </ul>
              </div>
            </div>
          )}
        </>
      ) : null}
    </section>
  );
}

export default GraphLens;
