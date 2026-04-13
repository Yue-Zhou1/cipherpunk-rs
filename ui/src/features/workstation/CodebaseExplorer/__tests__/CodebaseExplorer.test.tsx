import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mockLoadExplorerGraph = vi.fn();
let pixiRendererInstance: {
  canvas: HTMLCanvasElement;
  onNodeClick: (id: string) => void;
  onPaneClick: () => void;
} | null = null;

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

vi.mock("../pixi/PixiRenderer", () => ({
  PixiRenderer: vi.fn().mockImplementation((options: any) => {
    pixiRendererInstance = {
      canvas: options.canvas as HTMLCanvasElement,
      onNodeClick: options.onNodeClick as (id: string) => void,
      onPaneClick: options.onPaneClick as () => void,
    };
    return {
      init: vi.fn().mockResolvedValue(undefined),
      destroy: vi.fn(),
      resize: vi.fn(),
      updateGraph: vi.fn(),
    };
  }),
}));

vi.mock("pixi.js", () => ({
  Application: vi.fn(() => ({
    init: vi.fn().mockResolvedValue(undefined),
    destroy: vi.fn(),
    resize: vi.fn(),
    stage: {
      addChild: vi.fn(),
      addChildAt: vi.fn(),
      x: 0,
      y: 0,
      scale: { x: 1, y: 1, set: vi.fn() },
    },
    canvas: document.createElement("canvas"),
    ticker: { add: vi.fn() },
  })),
  Container: vi.fn(() => ({
    addChild: vi.fn(),
    addChildAt: vi.fn(),
    removeChild: vi.fn(),
    destroy: vi.fn(),
  })),
  Graphics: vi.fn(() => ({
    clear: vi.fn().mockReturnThis(),
    roundRect: vi.fn().mockReturnThis(),
    fill: vi.fn().mockReturnThis(),
    stroke: vi.fn().mockReturnThis(),
    moveTo: vi.fn().mockReturnThis(),
    bezierCurveTo: vi.fn().mockReturnThis(),
    lineTo: vi.fn().mockReturnThis(),
    alpha: 1,
    destroy: vi.fn(),
  })),
  Text: vi.fn(() => ({
    text: "",
    x: 0,
    y: 0,
    height: 16,
    alpha: 1,
    destroy: vi.fn(),
  })),
  TextStyle: vi.fn(),
}));

function simulateNodeClick(nodeId: string): void {
  if (!pixiRendererInstance) {
    throw new Error("PixiRenderer instance not ready");
  }
  act(() => {
    pixiRendererInstance!.onNodeClick(nodeId);
  });
}

function simulatePaneClick(): void {
  if (!pixiRendererInstance) {
    throw new Error("PixiRenderer instance not ready");
  }
  act(() => {
    pixiRendererInstance!.onPaneClick();
  });
}

async function waitForPixiRenderer(): Promise<void> {
  await waitFor(() => {
    expect(pixiRendererInstance).not.toBeNull();
  });
}

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
    pixiRendererInstance = null;
    mockLoadExplorerGraph.mockImplementation(
      (_sessionId: string, _depth?: "overview" | "full", cluster?: string) =>
        Promise.resolve(cluster ? MOCK_CLUSTER_RESPONSE : MOCK_OVERVIEW_RESPONSE)
    );
  });

  async function expandToSymbolLevel(): Promise<void> {
    await waitForPixiRenderer();
    simulateNodeClick("crt_001");
    await waitFor(() => {
      expect(mockLoadExplorerGraph).toHaveBeenCalledWith("test-session", undefined, "crt_001");
    });

    simulateNodeClick("mod_002");
    await waitFor(() => {
      expect(mockLoadExplorerGraph).toHaveBeenCalledWith("test-session", undefined, "mod_002");
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

  it("renders canvas after overview loads", async () => {
    render(<CodebaseExplorer sessionId="test-session" />);

    await waitFor(() => {
      expect(mockLoadExplorerGraph).toHaveBeenCalledWith("test-session", "overview");
    });
    expect(screen.getByLabelText("Codebase graph")).toBeInTheDocument();
    expect(screen.getByText("OVERVIEW")).toBeInTheDocument();
  });

  it("shows FOCUS state badge and context panel after node click", async () => {
    render(<CodebaseExplorer sessionId="test-session" />);

    await expandToSymbolLevel();
    simulateNodeClick("sym_004");

    await waitFor(() => {
      expect(screen.getByText("FOCUS")).toBeInTheDocument();
      expect(screen.getByLabelText("Node context")).toBeInTheDocument();
    });
  });

  it("shows ego banner with node name and counts when a node is focused", async () => {
    render(<CodebaseExplorer sessionId="test-session" />);

    await expandToSymbolLevel();
    simulateNodeClick("sym_004");

    await waitFor(() => {
      expect(screen.getByTestId("ego-banner")).toBeInTheDocument();
    });

    expect(screen.getByText(/← overview/i)).toBeInTheDocument();
    expect(screen.getByText(/callers:/i)).toBeInTheDocument();
    expect(screen.getByText(/callees:/i)).toBeInTheDocument();
  });

  it("returns to overview when ego banner back button is clicked", async () => {
    render(<CodebaseExplorer sessionId="test-session" />);

    await expandToSymbolLevel();
    simulateNodeClick("sym_004");

    await waitFor(() => {
      expect(screen.getByText(/← overview/i)).toBeInTheDocument();
    });

    fireEvent.click(screen.getByText(/← overview/i));

    await waitFor(() => {
      expect(screen.queryByText(/← overview/i)).not.toBeInTheDocument();
      expect(screen.getByText("OVERVIEW")).toBeInTheDocument();
    });
  });

  it("Esc returns from focus to overview", async () => {
    render(<CodebaseExplorer sessionId="test-session" />);

    await expandToSymbolLevel();
    simulateNodeClick("sym_004");

    await waitFor(() => {
      expect(screen.getByText("FOCUS")).toBeInTheDocument();
    });

    fireEvent.keyDown(window, { key: "Escape" });

    await waitFor(() => {
      expect(screen.getByText("OVERVIEW")).toBeInTheDocument();
    });
  });

  it("shows neighborhood highlight summary and Esc clears highlight", async () => {
    render(<CodebaseExplorer sessionId="test-session" />);

    await expandToSymbolLevel();
    simulateNodeClick("sym_004");

    await waitFor(() => {
      expect(screen.getByText("FOCUS")).toBeInTheDocument();
      expect(screen.getByLabelText("Node context")).toBeInTheDocument();
    });

    fireEvent.click(await screen.findByRole("button", { name: /show callees/i }));

    await waitFor(() => {
      expect(screen.getByText(/highlighting/i)).toBeInTheDocument();
    });

    fireEvent.keyDown(window, { key: "Escape" });

    await waitFor(() => {
      expect(screen.getByText("FOCUS")).toBeInTheDocument();
      expect(screen.queryByText(/highlighting/i)).toBeNull();
    });
  });

  it("clears focus on pane click callback", async () => {
    render(<CodebaseExplorer sessionId="test-session" />);

    await expandToSymbolLevel();
    simulateNodeClick("sym_004");

    await waitFor(() => {
      expect(screen.getByText("FOCUS")).toBeInTheDocument();
    });

    simulatePaneClick();

    await waitFor(() => {
      expect(screen.getByText("OVERVIEW")).toBeInTheDocument();
    });
  });
});
