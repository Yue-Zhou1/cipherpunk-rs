import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import type { ProjectTreeNode } from "../../ipc/commands";
import ProjectExplorer from "./ProjectExplorer";

describe("ProjectExplorer", () => {
  it("does not crash when backend omits empty children arrays", () => {
    const backendShapedNodes = [
      {
        name: "src",
        path: "src",
        kind: "directory",
      },
      {
        name: "README.md",
        path: "README.md",
        kind: "file",
      },
    ] as unknown as ProjectTreeNode[];

    expect(() =>
      render(
        <ProjectExplorer
          sessionId="sess-1"
          nodes={backendShapedNodes}
          selectedFilePath={null}
          onSelectFile={vi.fn()}
          isLoading={false}
          error={null}
        />
      )
    ).not.toThrow();

    expect(screen.getByText("src")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "README.md" })).toBeInTheDocument();
  });
});
