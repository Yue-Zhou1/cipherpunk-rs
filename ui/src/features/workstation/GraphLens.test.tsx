import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import GraphLens, { graphLensVariantFromEnv } from "./GraphLens";
import * as commands from "../../ipc/commands";

describe("GraphLens", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("selects graph implementation by transport mode", () => {
    expect(graphLensVariantFromEnv("http")).toBe("reactflow");
    expect(graphLensVariantFromEnv("tauri")).toBe("cytoscape");
    expect(graphLensVariantFromEnv(undefined)).toBe("cytoscape");
  });

  it("switches between graph lenses including symbol", async () => {
    render(<GraphLens sessionId="sess-1" />);

    fireEvent.change(await screen.findByLabelText(/select graph lens/i), {
      target: { value: "feature" },
    });
    expect(await screen.findByRole("heading", { name: /feature graph/i })).toBeInTheDocument();
    fireEvent.change(await screen.findByLabelText(/select graph lens/i), {
      target: { value: "symbol" },
    });
    expect(await screen.findByRole("heading", { name: /symbol graph/i })).toBeInTheDocument();
  });

  it("highlights selected nodes provided by review selection context", async () => {
    render(<GraphLens sessionId="sess-1" selectedNodeIds={["f2"]} />);

    expect(await screen.findByText(/review context selected 1 node/i)).toBeInTheDocument();
    const nodeLabel = await screen.findByText("crates/apps/tauri-ui/src/ipc.rs");
    expect(nodeLabel.closest("li")).toHaveClass("selected");
  });

  it("shows a no-data placeholder when graph loading fails", async () => {
    vi.spyOn(commands, "loadFileGraph").mockRejectedValueOnce(new Error("timeout"));

    render(<GraphLens sessionId="sess-1" />);

    expect(await screen.findByText("No graph data available")).toBeInTheDocument();
    expect(
      screen.getByText(/Run the BuildProjectIr job to generate the code graph/i)
    ).toBeInTheDocument();
  });

  it("shows an empty graph placeholder when graph has no nodes", async () => {
    vi.spyOn(commands, "loadFileGraph").mockResolvedValueOnce({
      sessionId: "sess-1",
      lens: "file",
      redactedValues: true,
      nodes: [],
      edges: [],
    });

    render(<GraphLens sessionId="sess-1" />);

    expect(
      await screen.findByText("Graph is empty - no source files found in the selected scope.")
    ).toBeInTheDocument();
  });

  it("filters graph nodes by search query and dims non-matches", async () => {
    render(<GraphLens sessionId="sess-1" />);

    fireEvent.change(await screen.findByPlaceholderText(/search/i), {
      target: { value: "session.rs" },
    });

    expect(await screen.findByText(/1 matches/i)).toBeInTheDocument();
    const dimmed = screen.getAllByTestId("graph-node-row").filter((row) =>
      row.classList.contains("search-dimmed")
    );
    expect(dimmed.length).toBeGreaterThan(0);
  });

  it("calls navigate callback with path and line when a symbol node is clicked", async () => {
    const navigateSpy = vi.fn();
    vi.spyOn(commands, "loadSymbolGraph").mockResolvedValueOnce({
      sessionId: "sess-1",
      lens: "symbol",
      redactedValues: true,
      nodes: [
        {
          id: "symbol:/tmp/repo/src/lib.rs::verify",
          label: "verify",
          kind: "function",
          filePath: "src/lib.rs",
          line: 42,
        },
      ],
      edges: [],
    });

    render(<GraphLens sessionId="sess-1" onNavigateToSource={navigateSpy} />);
    fireEvent.change(await screen.findByLabelText(/select graph lens/i), {
      target: { value: "symbol" },
    });
    fireEvent.click(await screen.findByRole("button", { name: /verify/i }));

    expect(navigateSpy).toHaveBeenCalledWith("src/lib.rs", 42);
  });

  it("shows finding count badges with severity styling", async () => {
    vi.spyOn(commands, "loadFileGraph").mockResolvedValueOnce({
      sessionId: "sess-1",
      lens: "file",
      redactedValues: true,
      nodes: [
        {
          id: "f1",
          label: "src/lib.rs",
          kind: "file",
          filePath: "src/lib.rs",
          findingCount: 3,
          maxSeverity: "critical",
        },
      ],
      edges: [],
    });

    render(<GraphLens sessionId="sess-1" />);

    const badge = await screen.findByTestId("graph-node-finding-badge");
    expect(badge).toHaveTextContent("3");
    expect(badge.className).toContain("severity-critical");
  });
});
