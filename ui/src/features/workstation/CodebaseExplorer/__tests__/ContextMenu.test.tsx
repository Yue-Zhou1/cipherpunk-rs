import { fireEvent, render, screen } from "@testing-library/react";
import { forwardRef, type HTMLAttributes, type ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { ContextMenu } from "../ContextMenu";

const mockCtx = {
  nodeMap: new Map([
    [
      "sym_1",
      {
        id: "sym_1",
        label: "verify_sig",
        kind: "function",
        filePath: "src/lib.rs",
        line: 42,
      },
    ],
  ]),
  graph: {
    nodes: [],
    edges: [
      { from: "sym_0", to: "sym_1", relation: "calls" },
      { from: "sym_1", to: "sym_2", relation: "calls" },
    ],
  },
  focusNode: vi.fn(),
  showCallers: vi.fn(),
  showCallees: vi.fn(),
  onNavigateToSource: vi.fn(),
};
const mockTraceCtx = {
  traceToEntryPoint: vi.fn(),
  traceDataflowForNode: vi.fn(),
};

vi.mock("../ExplorerContext", () => ({
  useExplorer: () => mockCtx,
}));

vi.mock("framer-motion", () => ({
  AnimatePresence: ({ children }: { children: ReactNode }) => <>{children}</>,
  motion: {
    div: forwardRef<HTMLDivElement, HTMLAttributes<HTMLDivElement>>(
      ({ children, ...props }, ref) => (
        <div ref={ref} {...props}>
          {children}
        </div>
      )
    ),
  },
}));

describe("ContextMenu", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("renders node name and file path in header", () => {
    render(
      <ContextMenu
        nodeId="sym_1"
        x={100}
        y={100}
        onClose={vi.fn()}
        traceCtx={mockTraceCtx}
      />
    );
    expect(screen.getByText("verify_sig")).toBeInTheDocument();
    expect(screen.getByText("src/lib.rs")).toBeInTheDocument();
  });

  it("shows caller and callee actions with counts", () => {
    render(
      <ContextMenu
        nodeId="sym_1"
        x={100}
        y={100}
        onClose={vi.fn()}
        traceCtx={mockTraceCtx}
      />
    );
    expect(screen.getByText(/show callers/i)).toBeInTheDocument();
    expect(screen.getByText(/show callees/i)).toBeInTheDocument();
    expect(screen.getAllByText("(1)")).toHaveLength(2);
  });

  it("calls showCallers and onClose when Show callers is clicked", () => {
    const onClose = vi.fn();
    render(
      <ContextMenu
        nodeId="sym_1"
        x={100}
        y={100}
        onClose={onClose}
        traceCtx={mockTraceCtx}
      />
    );
    fireEvent.click(screen.getByText(/show callers/i));
    expect(mockCtx.focusNode).toHaveBeenCalledWith("sym_1");
    expect(mockCtx.showCallers).toHaveBeenCalled();
    expect(onClose).toHaveBeenCalled();
  });

  it("calls showCallees and onClose when Show callees is clicked", () => {
    const onClose = vi.fn();
    render(
      <ContextMenu
        nodeId="sym_1"
        x={100}
        y={100}
        onClose={onClose}
        traceCtx={mockTraceCtx}
      />
    );
    fireEvent.click(screen.getByText(/show callees/i));
    expect(mockCtx.focusNode).toHaveBeenCalledWith("sym_1");
    expect(mockCtx.showCallees).toHaveBeenCalled();
    expect(onClose).toHaveBeenCalled();
  });

  it("calls traceToEntryPoint and onClose when Trace to entry is clicked", () => {
    const onClose = vi.fn();
    render(
      <ContextMenu
        nodeId="sym_1"
        x={100}
        y={100}
        onClose={onClose}
        traceCtx={mockTraceCtx}
      />
    );

    fireEvent.click(screen.getByText(/trace to entry/i));

    expect(mockTraceCtx.traceToEntryPoint).toHaveBeenCalledWith("sym_1");
    expect(onClose).toHaveBeenCalled();
  });

  it("calls traceDataflowForNode and onClose when Trace dataflow is clicked", () => {
    const onClose = vi.fn();
    render(
      <ContextMenu
        nodeId="sym_1"
        x={100}
        y={100}
        onClose={onClose}
        traceCtx={mockTraceCtx}
      />
    );

    fireEvent.click(screen.getByText(/trace dataflow/i));

    expect(mockTraceCtx.traceDataflowForNode).toHaveBeenCalledWith("sym_1");
    expect(onClose).toHaveBeenCalled();
  });

  it("calls onNavigateToSource when Open in editor is clicked", () => {
    const onClose = vi.fn();
    render(
      <ContextMenu
        nodeId="sym_1"
        x={100}
        y={100}
        onClose={onClose}
        traceCtx={mockTraceCtx}
      />
    );
    fireEvent.click(screen.getByText(/open in editor/i));
    expect(mockCtx.onNavigateToSource).toHaveBeenCalledWith("src/lib.rs", 42);
  });

  it("renders nothing when nodeId is null", () => {
    const { container } = render(
      <ContextMenu
        nodeId={null}
        x={0}
        y={0}
        onClose={vi.fn()}
        traceCtx={mockTraceCtx}
      />
    );
    expect(container.firstChild).toBeNull();
  });
});
