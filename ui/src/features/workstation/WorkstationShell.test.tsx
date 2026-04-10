import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import WorkstationShell from "./WorkstationShell";

const selectFileSpy = vi.fn();
let transportKind: "tauri" | "http" = "tauri";

vi.mock("@monaco-editor/react", () => ({
  default: () => null,
}));

vi.mock("../../ipc/transport", () => ({
  getTransport: () => ({ kind: transportKind }),
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

vi.mock("./CodeEditorPane", () => ({
  default: ({ filePath, targetLine }: { filePath?: string | null; targetLine?: number | null }) => (
    <section>
      <h2>Code Editor</h2>
      <p data-testid="code-editor-state">
        {filePath ?? "none"}@{targetLine ?? "none"}
      </p>
    </section>
  ),
}));

vi.mock("./CodebaseExplorer", () => ({
  default: ({
    sessionId,
    onNavigateToSource,
  }: {
    sessionId: string;
    onNavigateToSource?: (filePath: string, line?: number) => void;
  }) => (
    <section>
      <div data-testid="codebase-explorer-state">{sessionId}</div>
      <button
        type="button"
        onClick={() => onNavigateToSource?.("rollup-core/src/lib.rs", 12)}
      >
        Navigate from Graph
      </button>
    </section>
  ),
}));

vi.mock("./SecurityOverviewPanel", () => ({
  default: () => <div>Security Overview</div>,
}));

vi.mock("./ChecklistPanel", () => ({
  default: () => <div>Checklist Plan</div>,
}));

vi.mock("./AuditPlanPanel", () => ({
  default: () => <div>Audit Plan</div>,
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
    transportKind = "tauri";
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

    expect(selectFileSpy).toHaveBeenCalledWith("rollup-core/src/lib.rs");
  });

  it("does not match bare filename suffixes without a path boundary", () => {
    render(<WorkstationShell sessionId="sess-1" />);
    fireEvent.click(screen.getByRole("button", { name: /select ambiguous filename/i }));

    expect(selectFileSpy).not.toHaveBeenCalled();
  });

  it("shows graph view by default in http mode", () => {
    transportKind = "http";
    render(<WorkstationShell sessionId="sess-1" />);

    expect(screen.getByTestId("codebase-explorer-state")).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: /code editor/i })).not.toBeInTheDocument();
  });

  it("navigates to source without losing graph in http mode", () => {
    transportKind = "http";
    render(<WorkstationShell sessionId="sess-1" />);

    expect(screen.getByTestId("codebase-explorer-state")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /navigate from graph/i }));

    expect(selectFileSpy).toHaveBeenCalledWith("rollup-core/src/lib.rs");
    expect(screen.getByTestId("codebase-explorer-state")).toBeInTheDocument();
    expect(screen.getByTestId("code-editor-state")).toHaveTextContent("none@12");
  });
});

describe("WorkstationShell web mode view switching", () => {
  beforeEach(() => {
    transportKind = "http";
    selectFileSpy.mockClear();
  });

  it("defaults to graph view showing codebase explorer", () => {
    render(<WorkstationShell sessionId="sess-1" />);
    expect(screen.getByTestId("codebase-explorer-state")).toBeInTheDocument();
  });

  it("switches to editor view when Editor button is clicked", () => {
    render(<WorkstationShell sessionId="sess-1" />);
    fireEvent.click(screen.getByRole("button", { name: /^editor$/i }));
    expect(screen.getByRole("heading", { name: /code editor/i })).toBeInTheDocument();
  });

  it("switches to info view when Info button is clicked", () => {
    render(<WorkstationShell sessionId="sess-1" />);
    expect(screen.queryByRole("tab", { name: /security overview/i })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /^info$/i }));
    expect(screen.getByRole("tab", { name: /security overview/i })).toBeInTheDocument();
  });
});
