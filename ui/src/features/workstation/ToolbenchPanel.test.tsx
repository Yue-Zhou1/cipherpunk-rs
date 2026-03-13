import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import ToolbenchPanel from "./ToolbenchPanel";

describe("ToolbenchPanel", () => {
  it("shows checklist and helper tools for the selected symbol", async () => {
    render(<ToolbenchPanel sessionId="sess-1" selection={{ kind: "symbol", id: "prove" }} />);

    expect(await screen.findByText(/kani/i)).toBeInTheDocument();
    expect(await screen.findByText(/domain checklists/i)).toBeInTheDocument();
  });
});
