import { render, screen } from "@testing-library/react";
import { describe, it, vi } from "vitest";

import WorkstationShell from "./WorkstationShell";

vi.mock("@monaco-editor/react", () => ({
  default: () => null,
}));

vi.mock("./useSessionState", () => ({
  default: () => ({
    projectTree: [],
    selectedFilePath: null,
    fileContent: "",
    consoleEntries: [],
    treeLoading: false,
    fileLoading: false,
    treeError: null,
    fileError: null,
    selectFile: () => undefined,
  }),
}));

vi.mock("./GraphLens", () => ({
  default: () => <div>Graph Lens</div>,
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
  default: () => <div>Review Queue</div>,
}));

describe("WorkstationShell", () => {
  it("renders explorer, editor, toolbench, and console panels", () => {
    render(<WorkstationShell sessionId="sess-1" />);

    expect(screen.getByRole("heading", { name: /project explorer/i })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: /toolbench/i })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: /activity console/i })).toBeInTheDocument();
  });
});
