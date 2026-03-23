import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import AuditPlanPanel from "./AuditPlanPanel";

describe("AuditPlanPanel", () => {
  it("renders generated plan sections", async () => {
    render(<AuditPlanPanel sessionId="sess-1" />);

    expect(await screen.findByRole("heading", { name: /audit plan/i })).toBeInTheDocument();
    expect(await screen.findByRole("button", { name: /architecture overview/i })).toBeInTheDocument();
    expect(await screen.findByRole("button", { name: /analysis domains/i })).toBeInTheDocument();
  });
});
