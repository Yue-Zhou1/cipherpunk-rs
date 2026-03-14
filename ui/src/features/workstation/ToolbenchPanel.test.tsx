import { render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import * as commands from "../../ipc/commands";
import ToolbenchPanel from "./ToolbenchPanel";

afterEach(() => {
  vi.restoreAllMocks();
});

describe("ToolbenchPanel", () => {
  it("shows checklist and helper tools for the selected symbol", async () => {
    render(<ToolbenchPanel sessionId="sess-1" selection={{ kind: "symbol", id: "prove" }} />);

    expect(await screen.findByText(/kani/i)).toBeInTheDocument();
    expect(await screen.findByText(/domain checklists/i)).toBeInTheDocument();
  });

  it("does not refetch when selection values are unchanged across rerenders", async () => {
    const spy = vi.spyOn(commands, "loadToolbenchContext").mockResolvedValue({
      sessionId: "sess-1",
      selection: { kind: "symbol", id: "prove" },
      recommendedTools: [{ toolId: "Kani", rationale: "baseline" }],
      domains: [{ id: "zk", rationale: "coverage" }],
      overviewNotes: ["note"],
      similarCases: [{ id: "case-1", title: "prior", summary: "prior summary" }],
    });

    const { rerender } = render(
      <ToolbenchPanel sessionId="sess-1" selection={{ kind: "symbol", id: "prove" }} />
    );

    await screen.findByText(/kani/i);

    rerender(<ToolbenchPanel sessionId="sess-1" selection={{ kind: "symbol", id: "prove" }} />);

    await waitFor(() => {
      expect(spy).toHaveBeenCalledTimes(1);
    });
  });
});
