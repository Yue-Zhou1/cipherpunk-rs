import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

vi.mock("reactflow", () => ({
  Handle: ({ type }: { type: string }) => <div data-testid={`handle-${type}`} />,
  Position: { Top: "top", Bottom: "bottom" },
}));

vi.mock("../ExplorerContext", () => ({
  useExplorer: () => ({
    clusterErrors: new Map<string, string>(),
    loadingClusters: new Set<string>(),
  }),
}));

import { ClusterNode } from "../nodes/ClusterNode";
import { FileNode } from "../nodes/FileNode";
import { SymbolNode } from "../nodes/SymbolNode";

describe("ClusterNode", () => {
  it("renders module name and child count", () => {
    render(
      <ClusterNode
        {...({
          id: "crt_1",
          data: { label: "engine-crypto", childCount: 12, expanded: false, kind: "crate" },
        } as any)}
      />
    );
    expect(screen.getByText("engine-crypto")).toBeTruthy();
    expect(screen.getByText("12")).toBeTruthy();
  });

  it("shows expand indicator when collapsed", () => {
    render(
      <ClusterNode
        {...({
          id: "mod_1",
          data: { label: "intake", childCount: 5, expanded: false, kind: "module" },
        } as any)}
      />
    );
    expect(screen.getByLabelText("expand")).toBeTruthy();
  });
});

describe("FileNode", () => {
  it("renders filename", () => {
    render(<FileNode data={{ label: "sig.rs" }} />);
    expect(screen.getByText("sig.rs")).toBeTruthy();
  });
});

describe("SymbolNode", () => {
  it("renders function name and signature", () => {
    render(
      <SymbolNode
        data={{
          label: "verify_signature",
          kind: "function",
          signature: {
            parameters: [
              { name: "msg", typeAnnotation: "&[u8]", position: 0 },
              { name: "sig", typeAnnotation: "&Signature", position: 1 },
            ],
            returnType: "Result<bool>",
          },
        }}
      />
    );
    expect(screen.getByText("verify_signature")).toBeTruthy();
    expect(screen.getByText("msg")).toBeTruthy();
    expect(screen.getByText(": &[u8]")).toBeTruthy();
    expect(screen.getByText("Result<bool>")).toBeTruthy();
  });

  it("renders signature elements as read-only", () => {
    render(
      <SymbolNode
        data={{
          label: "hash",
          kind: "function",
          signature: {
            parameters: [{ name: "data", typeAnnotation: "&[u8]", position: 0 }],
            returnType: "Hash",
          },
        }}
      />
    );
    expect(screen.queryByRole("button", { name: "data" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Hash" })).toBeNull();
  });
});
