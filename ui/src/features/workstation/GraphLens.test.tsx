import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import GraphLens, { graphLensVariantFromEnv } from "./GraphLens";

describe("GraphLens", () => {
  it("selects graph implementation by transport mode", () => {
    expect(graphLensVariantFromEnv("http")).toBe("reactflow");
    expect(graphLensVariantFromEnv("tauri")).toBe("cytoscape");
    expect(graphLensVariantFromEnv(undefined)).toBe("cytoscape");
  });

  it("switches between file, feature, and dataflow graph lenses", async () => {
    render(<GraphLens sessionId="sess-1" />);

    fireEvent.click(await screen.findByRole("tab", { name: /feature graph/i }));
    expect(await screen.findByRole("heading", { name: /feature graph/i })).toBeInTheDocument();
  });

  it("highlights selected nodes provided by review selection context", async () => {
    render(<GraphLens sessionId="sess-1" selectedNodeIds={["f2"]} />);

    expect(await screen.findByText(/review context selected 1 node/i)).toBeInTheDocument();
    const nodeLabel = await screen.findByText("crates/apps/tauri-ui/src/ipc.rs");
    expect(nodeLabel.closest("li")).toHaveClass("selected");
  });
});
