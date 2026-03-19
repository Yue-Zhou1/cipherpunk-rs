import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import WorkstationShell from "./WorkstationShell";

const selectFileSpy = vi.fn();

vi.mock("@monaco-editor/react", () => ({
  default: () => null,
}));

vi.mock("./useSessionState", () => ({
  default: () => ({
    projectTree: [
      {
        name: "rollup-core",
        path: "rollup-core",
        kind: "directory",
        children: [
          {
            name: "lib.rs",
            path: "rollup-core/src/lib.rs",
            kind: "file",
            children: [],
          },
        ],
      },
    ],
    selectedFilePath: null,
    fileContent: "",
    consoleEntries: [],
    treeLoading: false,
    fileLoading: false,
    treeError: null,
    fileError: null,
    selectFile: selectFileSpy,
  }),
}));

vi.mock("./GraphLens", () => ({
  default: ({ selectedNodeIds }: { selectedNodeIds?: string[] }) => (
    <div data-testid="graph-selection-state">{(selectedNodeIds ?? []).join("|")}</div>
  ),
}));

vi.mock("./SecurityOverviewPanel", () => ({
  default: () => <div>Security Overview</div>,
}));

vi.mock("./ChecklistPanel", () => ({
  default: () => <div>Checklist Plan</div>,
}));

vi.mock("./ToolbenchPanel", () => ({
  default: () => (
    <section>
      <h2>Toolbench</h2>
    </section>
  ),
}));

vi.mock("./ReviewQueue", () => ({
  default: ({ onSelectRecord }: { onSelectRecord?: (item: unknown) => void }) => (
    <section>
      <h2>Review Queue</h2>
      <button
        type="button"
        onClick={() =>
          onSelectRecord?.({
            recordId: "cand-1",
            kind: "candidate",
            title: "Candidate 1",
            summary: "summary",
            verificationStatus: "unverified",
            labels: [],
            evidenceRefs: [],
            irNodeIds: ["file:/tmp/repo/rollup-core/src/lib.rs", "symbol:/tmp/repo/rollup-core/src/lib.rs::verify"],
          })
        }
      >
        Select Review Item
      </button>
      <button
        type="button"
        onClick={() =>
          onSelectRecord?.({
            recordId: "cand-2",
            kind: "candidate",
            title: "Candidate 2",
            summary: "summary",
            verificationStatus: "unverified",
            labels: [],
            evidenceRefs: [],
            irNodeIds: ["file:/tmp/repo/pkg/mylib.rs"],
          })
        }
      >
        Select Ambiguous Filename
      </button>
    </section>
  ),
}));

describe("WorkstationShell", () => {
  beforeEach(() => {
    selectFileSpy.mockClear();
  });

  it("renders explorer, editor, toolbench, and console panels", () => {
    render(<WorkstationShell sessionId="sess-1" />);

    expect(screen.getByRole("heading", { name: /project explorer/i })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: /toolbench/i })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: /activity console/i })).toBeInTheDocument();
  });

  it("syncs review selection into graph highlighting and editor navigation", () => {
    render(<WorkstationShell sessionId="sess-1" />);
    fireEvent.click(screen.getByRole("button", { name: /select review item/i }));

    expect(screen.getByTestId("graph-selection-state").textContent).toContain(
      "file:/tmp/repo/rollup-core/src/lib.rs"
    );
    expect(selectFileSpy).toHaveBeenCalledWith("rollup-core/src/lib.rs");
  });

  it("does not match bare filename suffixes without a path boundary", () => {
    render(<WorkstationShell sessionId="sess-1" />);
    fireEvent.click(screen.getByRole("button", { name: /select ambiguous filename/i }));

    expect(selectFileSpy).not.toHaveBeenCalled();
  });
});
