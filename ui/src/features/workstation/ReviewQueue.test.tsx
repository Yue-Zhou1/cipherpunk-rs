import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import ReviewQueue from "./ReviewQueue";

describe("ReviewQueue", () => {
  it("allows the engineer to confirm or reject a candidate", async () => {
    render(<ReviewQueue sessionId="sess-1" />);

    expect(await screen.findByRole("button", { name: /confirm finding/i })).toBeInTheDocument();
    expect(await screen.findByRole("button", { name: /mark false positive/i })).toBeInTheDocument();
  });

  it("selecting a review item publishes graph-aware selection payload", async () => {
    const onSelectRecord = vi.fn();
    render(<ReviewQueue sessionId="sess-1" onSelectRecord={onSelectRecord} />);

    const selectButton = await screen.findByRole("button", {
      name: /potential signer replay path/i,
    });
    fireEvent.click(selectButton);

    expect(onSelectRecord).toHaveBeenCalled();
    expect(onSelectRecord.mock.calls.at(-1)?.[0]?.irNodeIds).toBeDefined();
  });
});
