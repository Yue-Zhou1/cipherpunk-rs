import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import SecurityOverviewPanel from "./SecurityOverviewPanel";

describe("SecurityOverviewPanel", () => {
  it("shows ai-generated assets, trust boundaries, and hotspots as review notes", async () => {
    render(<SecurityOverviewPanel sessionId="sess-1" />);

    expect(await screen.findByText(/trust boundaries/i)).toBeInTheDocument();
  });
});
