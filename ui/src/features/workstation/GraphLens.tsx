import { Suspense, lazy } from "react";

import type { GraphLensProps } from "./GraphLensCytoscape";

export function graphLensVariantFromEnv(
  transport: string | undefined
): "reactflow" | "cytoscape" {
  return transport === "http" ? "reactflow" : "cytoscape";
}

const transportMode = (
  import.meta as unknown as { env?: { VITE_TRANSPORT?: string } }
).env?.VITE_TRANSPORT;
const implementation = graphLensVariantFromEnv(transportMode);

const GraphLensImpl =
  implementation === "reactflow"
    ? lazy(() => import("./GraphLensReactFlow"))
    : lazy(() => import("./GraphLensCytoscape"));

function GraphLens(props: GraphLensProps): JSX.Element {
  return (
    <Suspense
      fallback={
        <section className="panel workstation-graph-lens" aria-label="Graph Lens">
          <p className="muted-text">Loading graph lens...</p>
        </section>
      }
    >
      <GraphLensImpl {...props} />
    </Suspense>
  );
}

export default GraphLens;
