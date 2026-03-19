import { Suspense, lazy, useEffect, useMemo, useState } from "react";
import type { ElementDefinition } from "cytoscape";

import {
  loadDataflowGraph,
  loadFeatureGraph,
  loadFileGraph,
  type GraphLensKind,
  type ProjectGraphResponse,
} from "../../ipc/commands";

type GraphLensProps = {
  sessionId: string;
  selectedNodeIds?: string[];
};

const LENS_OPTIONS: Array<{ kind: GraphLensKind; label: string }> = [
  { kind: "file", label: "File Graph" },
  { kind: "feature", label: "Feature Graph" },
  { kind: "dataflow", label: "Dataflow Graph" },
];
const CytoscapeComponent = lazy(() => import("react-cytoscapejs"));

function GraphLens({ sessionId, selectedNodeIds = [] }: GraphLensProps): JSX.Element {
  const [lens, setLens] = useState<GraphLensKind>("file");
  const [includeValues, setIncludeValues] = useState(false);
  const [graph, setGraph] = useState<ProjectGraphResponse | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setIsLoading(true);
    setError(null);

    const request =
      lens === "file"
        ? loadFileGraph(sessionId)
        : lens === "feature"
          ? loadFeatureGraph(sessionId)
          : loadDataflowGraph(sessionId, includeValues);

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
  const elements = useMemo<ElementDefinition[]>(
    () =>
      graph
        ? [
            ...graph.nodes.map((node) => ({
              data: {
                id: node.id,
                label: node.label,
                kind: node.kind,
              },
              classes: selectedNodeIdSet.has(node.id) ? "selected" : "",
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
    [graph, selectedNodeIdSet]
  );

  return (
    <section className="panel workstation-graph-lens" aria-label="Graph Lens">
      <div className="workstation-panel-head">
        <p className="workstation-panel-eyebrow">Graph Lens</p>
        <h2>{title}</h2>
      </div>

      <div className="graph-lens-tabs" role="tablist" aria-label="Graph lens selector">
        {LENS_OPTIONS.map((entry) => (
          <button
            key={entry.kind}
            type="button"
            className={`graph-lens-tab${lens === entry.kind ? " active" : ""}`}
            onClick={() => setLens(entry.kind)}
            role="tab"
            aria-selected={lens === entry.kind}
          >
            {entry.label}
          </button>
        ))}
      </div>

      {lens === "dataflow" ? (
        <div className="banner banner-info graph-lens-redaction">
          <span>Value previews are redacted by default.</span>
          <button
            type="button"
            className="inline-action"
            onClick={() => setIncludeValues((value) => !value)}
          >
            {includeValues ? "Hide Value Previews" : "Approve Value Previews"}
          </button>
        </div>
      ) : null}

      {isLoading ? <p className="muted-text">Loading graph...</p> : null}
      {error ? <p className="banner banner-error">{error}</p> : null}
      {selectedNodeIds.length > 0 ? (
        <p className="muted-text">Review context selected {selectedNodeIds.length} node(s).</p>
      ) : null}

      {!isLoading && !error && graph ? (
        <>
          <p className="muted-text">
            {graph.nodes.length} nodes / {graph.edges.length} edges
          </p>
          {!isJsdom ? (
            <div className="graph-lens-canvas">
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
                  ]}
                  style={{ width: "100%", height: "100%" }}
                  minZoom={0.2}
                  maxZoom={2.4}
                  cy={(cy) => {
                    // This runs whenever the Cytoscape instance is (re)created, so a selected
                    // review context will refit on graph reload/lens switch as well as selection.
                    if (selectedNodeIds.length === 0) {
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
            </div>
          ) : (
            <div className="graph-lens-grid">
              <div className="graph-lens-block">
                <h3>Nodes</h3>
                <ul>
                  {graph.nodes.slice(0, 8).map((node) => (
                    <li
                      key={node.id}
                      className={selectedNodeIdSet.has(node.id) ? "selected" : undefined}
                    >
                      <code>{node.label}</code>
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
