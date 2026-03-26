import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

vi.mock("reactflow", () => ({
  Handle: ({ type }: { type: string }) => <div data-testid={`handle-${type}`} />,
  Position: { Top: "top", Bottom: "bottom" },
}));

import { ClusterNode } from "../nodes/ClusterNode";
import { FileNode } from "../nodes/FileNode";
import { SymbolNode } from "../nodes/SymbolNode";

describe("ClusterNode", () => {
  it("renders module name and child count", () => {
    render(
      <ClusterNode data={{ label: "engine-crypto", childCount: 12, expanded: false, kind: "crate" }} />
    );
    expect(screen.getByText("engine-crypto")).toBeTruthy();
    expect(screen.getByText("12")).toBeTruthy();
  });

  it("shows expand indicator when collapsed", () => {
    render(<ClusterNode data={{ label: "intake", childCount: 5, expanded: false, kind: "module" }} />);
    expect(screen.getByLabelText("expand")).toBeTruthy();
  });
});

describe("FileNode", () => {
  it("renders filename", () => {
    render(<FileNode data={{ label: "sig.rs", language: "rust" }} />);
    expect(screen.getByText("sig.rs")).toBeTruthy();
  });
});

describe("SymbolNode", () => {
  it("renders function name and signature", () => {
    render(
      <SymbolNode
        data={{
          label: "verify_signature",
          signature: {
            parameters: [
              { name: "msg", typeAnnotation: "&[u8]", position: 0 },
              { name: "sig", typeAnnotation: "&Signature", position: 1 },
            ],
            returnType: "Result<bool>",
          },
          onParameterClick: () => {},
          onReturnClick: () => {},
        }}
      />
    );
    expect(screen.getByText("verify_signature")).toBeTruthy();
    expect(screen.getByText("msg")).toBeTruthy();
    expect(screen.getByText(": &[u8]")).toBeTruthy();
    expect(screen.getByText("Result<bool>")).toBeTruthy();
  });

  it("calls onParameterClick when a parameter is clicked", () => {
    const onClick = vi.fn();
    render(
      <SymbolNode
        data={{
          label: "hash",
          signature: {
            parameters: [{ name: "data", typeAnnotation: "&[u8]", position: 0 }],
            returnType: "Hash",
          },
          onParameterClick: onClick,
          onReturnClick: () => {},
        }}
      />
    );
    fireEvent.click(screen.getByText("data"));
    expect(onClick).toHaveBeenCalledWith("data");
  });

  it("calls onReturnClick when return type is clicked", () => {
    const onClick = vi.fn();
    render(
      <SymbolNode
        data={{
          label: "hash",
          signature: {
            parameters: [],
            returnType: "Hash",
          },
          onParameterClick: () => {},
          onReturnClick: onClick,
        }}
      />
    );
    fireEvent.click(screen.getByText("Hash"));
    expect(onClick).toHaveBeenCalled();
  });
});
