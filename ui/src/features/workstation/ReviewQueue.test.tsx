import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import ReviewQueue from "./ReviewQueue";

describe("ReviewQueue", () => {
  it("allows the engineer to confirm or reject a candidate", async () => {
    render(<ReviewQueue sessionId="sess-1" />);

    expect(await screen.findByRole("button", { name: /confirm finding/i })).toBeInTheDocument();
    expect(await screen.findByRole("button", { name: /mark false positive/i })).toBeInTheDocument();
  });
});
