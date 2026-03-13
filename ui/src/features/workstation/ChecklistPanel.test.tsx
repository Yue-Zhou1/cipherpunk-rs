import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import ChecklistPanel from "./ChecklistPanel";

describe("ChecklistPanel", () => {
  it("renders planned checklist domains", async () => {
    render(<ChecklistPanel sessionId="sess-1" />);

    expect(await screen.findByText(/checklist plan/i)).toBeInTheDocument();
  });
});
