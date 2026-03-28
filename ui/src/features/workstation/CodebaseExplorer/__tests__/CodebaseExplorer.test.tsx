import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, beforeEach, vi } from "vitest";

const mockLoadExplorerGraph = vi.fn();

vi.mock("../../../../ipc/commands", () => ({
  loadExplorerGraph: (...args: unknown[]) => mockLoadExplorerGraph(...args),
}));

vi.mock("../../../../ipc/transport", () => ({
  getTransport: () => ({
    subscribe: vi.fn(() => vi.fn()),
  }),
}));

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

const MOCK_OVERVIEW_RESPONSE = {
  sessionId: "test-session",
  nodes: [
    { id: "crt_001", label: "engine-crypto", kind: "crate", childCount: 3 },
    { id: "mod_002", label: "src", kind: "module", childCount: 3 },
    { id: "fil_003", label: "verify.rs", kind: "file", filePath: "engine-crypto/src/verify.rs" },
  ],
  edges: [
    { from: "crt_001", to: "mod_002", relation: "contains" },
    { from: "mod_002", to: "fil_003", relation: "contains" },
  ],
};

const MOCK_CLUSTER_RESPONSE = {
  sessionId: "test-session",
  nodes: [
    {
      id: "sym_004",
      label: "verify_signature",
      kind: "function",
      filePath: "engine-crypto/src/verify.rs",
      line: 42,
      signature: {
        parameters: [{ name: "msg", typeAnnotation: "&[u8]", position: 0 }],
        returnType: "bool",
      },
    },
    {
      id: "sym_005",
      label: "hash_blake3",
      kind: "function",
      filePath: "engine-crypto/src/hash.rs",
      line: 18,
    },
  ],
  edges: [
    { from: "fil_003", to: "sym_004", relation: "contains" },
    { from: "fil_003", to: "sym_005", relation: "contains" },
    { from: "sym_004", to: "sym_005", relation: "calls" },
    {
      from: "sym_005",
      to: "sym_004",
      relation: "parameter_flow",
      parameterName: "msg",
      parameterPosition: 0,
    },
  ],
};

describe("CodebaseExplorer", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockLoadExplorerGraph.mockImplementation(
      (_sessionId: string, _depth?: "overview" | "full", cluster?: string) =>
        Promise.resolve(cluster ? MOCK_CLUSTER_RESPONSE : MOCK_OVERVIEW_RESPONSE)
    );
  });

  async function expandToSymbolLevel(): Promise<void> {
    const granularity = await screen.findByLabelText("View granularity");
    fireEvent.change(granularity, { target: { value: "crates" } });
    fireEvent.click(await screen.findByText("engine-crypto"));
    fireEvent.click(await screen.findByText("src"));
    await waitFor(() => {
      expect(screen.getByText("verify_signature")).toBeInTheDocument();
    });
  }

  it("shows loading state initially", () => {
    mockLoadExplorerGraph.mockReturnValue(new Promise(() => {}));

    render(<CodebaseExplorer sessionId="test-session" />);
    expect(screen.getByText("Loading project graph...")).toBeInTheDocument();
  });

  it("shows error state on API failure", async () => {
    mockLoadExplorerGraph.mockRejectedValue(new Error("Connection refused"));

    render(<CodebaseExplorer sessionId="test-session" />);
    await waitFor(() => {
      expect(screen.getByText("Connection refused")).toBeInTheDocument();
    });
    expect(screen.getByText("Retry")).toBeInTheDocument();
  });

  it("renders graph nodes after overview loads", async () => {
    render(<CodebaseExplorer sessionId="test-session" />);

    await waitFor(() => {
      expect(screen.getByText("verify.rs")).toBeInTheDocument();
    });
    expect(mockLoadExplorerGraph).toHaveBeenCalledWith("test-session", "overview");
    expect(screen.queryByText("verify_signature")).toBeNull();
  });

  it("shows FOCUS state badge and context panel after node click", async () => {
    render(<CodebaseExplorer sessionId="test-session" />);

    await expandToSymbolLevel();
    fireEvent.click(await screen.findByText("verify_signature"));

    await waitFor(() => {
      expect(screen.getByText("FOCUS")).toBeInTheDocument();
      expect(screen.getByLabelText("Node context")).toBeInTheDocument();
    });
    expect(mockLoadExplorerGraph).toHaveBeenCalledWith("test-session", undefined, "crt_001");
    expect(mockLoadExplorerGraph).toHaveBeenCalledWith("test-session", undefined, "mod_002");
  });

  it("Esc returns from focus to overview", async () => {
    render(<CodebaseExplorer sessionId="test-session" />);

    await expandToSymbolLevel();
    fireEvent.click(await screen.findByText("verify_signature"));

    await waitFor(() => {
      expect(screen.getByText("FOCUS")).toBeInTheDocument();
    });

    fireEvent.keyDown(window, { key: "Escape" });

    await waitFor(() => {
      expect(screen.getByText("OVERVIEW")).toBeInTheDocument();
    });
  });

  it("transitions OVERVIEW -> FOCUS -> TRACE and Esc returns to FOCUS", async () => {
    render(<CodebaseExplorer sessionId="test-session" />);

    await expandToSymbolLevel();
    fireEvent.click(await screen.findByText("verify_signature"));

    await waitFor(() => {
      expect(screen.getByText("FOCUS")).toBeInTheDocument();
      expect(screen.getByLabelText("Node context")).toBeInTheDocument();
    });

    fireEvent.click(await screen.findByTitle("Trace origin of msg"));

    await waitFor(() => {
      expect(screen.getByText("TRACE")).toBeInTheDocument();
    });

    fireEvent.keyDown(window, { key: "Escape" });

    await waitFor(() => {
      expect(screen.getByText("FOCUS")).toBeInTheDocument();
      expect(screen.queryByText("TRACE")).toBeNull();
    });
  });
});
