import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

vi.mock("elkjs/lib/elk.bundled.js", () => ({
  default: class MockElk {
    async layout(graph: { children?: Array<{ id: string }> }) {
      return {
        children: (graph.children ?? []).map((child, index) => ({
          id: child.id,
          x: (index % 8) * 180,
          y: Math.floor(index / 8) * 120,
        })),
      };
    }
  },
}));

vi.mock("reactflow", () => {
  const ReactFlow = ({ nodes, onNodeClick, onPaneClick, children, onInit }: any) => (
    <div
      data-testid="mock-reactflow"
      onClick={(event) => onPaneClick?.(event)}
      ref={() => {
        onInit?.({});
      }}
    >
      {nodes?.map((node: any) => (
        <button
          key={node.id}
          data-testid={`rf-node-${node.id}`}
          onClick={(event) => {
            event.stopPropagation();
            onNodeClick?.(event, node);
          }}
          type="button"
        >
          {node.data?.label ?? node.id}
        </button>
      ))}
      {children}
    </div>
  );

  return {
    __esModule: true,
    default: ReactFlow,
    Background: () => null,
    Controls: () => null,
    MiniMap: () => null,
    Handle: ({ type }: { type: string }) => <span data-testid={`handle-${type}`} />,
    Position: { Top: "top", Bottom: "bottom" },
    MarkerType: { ArrowClosed: "arrowclosed" },
  };
});

import CodebaseExplorer from "../index";

describe("CodebaseExplorer", () => {
  it("renders in overview state with toolbar", () => {
    render(<CodebaseExplorer sessionId="test-session" />);
    expect(screen.getByLabelText("Explorer controls")).toBeTruthy();
    expect(screen.getByLabelText("View granularity")).toBeTruthy();
    expect(screen.getByLabelText("Search nodes")).toBeTruthy();
    expect(screen.getByText("OVERVIEW")).toBeTruthy();
  });

  it("renders graph nodes from fixture data", () => {
    render(<CodebaseExplorer sessionId="test-session" />);
    expect(screen.getByLabelText("Codebase graph")).toBeTruthy();
  });

  it("does not show context panel in overview state", () => {
    render(<CodebaseExplorer sessionId="test-session" />);
    expect(screen.queryByLabelText("Node context")).toBeNull();
  });

  it("search input works and shows match count", async () => {
    render(<CodebaseExplorer sessionId="test-session" />);
    const search = screen.getByLabelText("Search nodes");
    fireEvent.change(search, { target: { value: "verify" } });
    expect(await screen.findByText(/matches/)).toBeTruthy();
  });

  it("granularity dropdown has all options", () => {
    render(<CodebaseExplorer sessionId="test-session" />);
    const select = screen.getByLabelText("View granularity");
    expect(select).toBeTruthy();
    const options = select.querySelectorAll("option");
    expect(options.length).toBe(4);
  });

  it("depth control not visible in overview", () => {
    render(<CodebaseExplorer sessionId="test-session" />);
    expect(screen.queryByLabelText("Depth control")).toBeNull();
  });

  it("shows FOCUS state badge and context panel after node click", async () => {
    render(<CodebaseExplorer sessionId="test-session" />);
    const granularity = screen.getByLabelText("View granularity");
    fireEvent.change(granularity, { target: { value: "files" } });
    const [target] = await screen.findAllByText(
      /verify_signature|hash_blake3|parse_submission/i
    );
    fireEvent.click(target);

    await waitFor(() => {
      expect(screen.getByText("FOCUS")).toBeTruthy();
      expect(screen.getByLabelText("Node context")).toBeTruthy();
    });
  });

  it("Esc returns from focus to overview", async () => {
    render(<CodebaseExplorer sessionId="test-session" />);
    const granularity = screen.getByLabelText("View granularity");
    fireEvent.change(granularity, { target: { value: "files" } });
    const [target] = await screen.findAllByText(
      /verify_signature|hash_blake3|parse_submission/i
    );
    fireEvent.click(target);

    await waitFor(() => {
      expect(screen.getByText("FOCUS")).toBeTruthy();
    });

    fireEvent.keyDown(window, { key: "Escape" });

    await waitFor(() => {
      expect(screen.getByText("OVERVIEW")).toBeTruthy();
    });
  });

  it("transitions OVERVIEW -> FOCUS -> TRACE and Esc returns to FOCUS", async () => {
    render(<CodebaseExplorer sessionId="test-session" />);

    fireEvent.change(screen.getByLabelText("View granularity"), {
      target: { value: "files" },
    });

    fireEvent.click(
      await screen.findByTestId(
        "rf-node-symbol:crates/engine-crypto/src/signature/verify.rs::verify_signature"
      )
    );

    await waitFor(() => {
      expect(screen.getByText("FOCUS")).toBeTruthy();
      expect(screen.getByLabelText("Node context")).toBeTruthy();
    });

    fireEvent.click(await screen.findByTitle("Trace origin of msg"));

    await waitFor(() => {
      expect(screen.getByText("TRACE")).toBeTruthy();
    });

    fireEvent.keyDown(window, { key: "Escape" });

    await waitFor(() => {
      expect(screen.getByText("FOCUS")).toBeTruthy();
      expect(screen.queryByText("TRACE")).toBeNull();
    });
  });
});
