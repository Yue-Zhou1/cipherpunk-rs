import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import * as commands from "../../ipc/commands";
import ActivityConsole from "./ActivityConsole";

afterEach(() => {
  vi.restoreAllMocks();
});

describe("ActivityConsole", () => {
  it("shows filter tabs with summary counts and filters visible rows", async () => {
    vi.spyOn(commands, "loadActivitySummary").mockResolvedValue({
      sessionId: "sess-1",
      llmCalls: [
        {
          role: "SearchHints",
          count: 1,
          avgDurationMs: 40,
          totalPromptChars: 20,
          totalResponseChars: 30,
          providersUsed: ["openai"],
          succeeded: 1,
          failed: 0,
        },
      ],
      toolActions: [
        {
          toolFamily: "kani",
          count: 1,
          succeeded: 1,
          failed: 0,
          avgDurationMs: 80,
        },
      ],
      reviewDecisions: [{ action: "confirm", count: 1 }],
      engineOutcomes: [{ engine: "crypto_zk", status: "completed", findingsCount: 2, durationMs: 60 }],
      totalEvents: 4,
      totalDurationMs: 180,
    });

    render(
      <ActivityConsole
        sessionId="sess-1"
        entries={[
          { timestamp: "10:00:00", source: "llm.interaction", level: "info", message: "llm event" },
          {
            timestamp: "10:00:01",
            source: "tool.action.completed",
            level: "info",
            message: "tool event",
          },
          {
            timestamp: "10:00:02",
            source: "review.decision",
            level: "warning",
            message: "review event",
          },
        ]}
      />
    );

    expect(await screen.findByRole("button", { name: /LLM/i })).toBeInTheDocument();
    expect(await screen.findByRole("button", { name: /Tools/i })).toBeInTheDocument();
    expect(await screen.findByRole("button", { name: /Reviews/i })).toBeInTheDocument();
    expect(await screen.findByText(/1 LLM calls/i)).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /LLM/i }));
    await waitFor(() => {
      expect(screen.getByText(/llm event/i)).toBeInTheDocument();
      expect(screen.queryByText(/tool event/i)).not.toBeInTheDocument();
      expect(screen.queryByText(/review event/i)).not.toBeInTheDocument();
    });
  });
});
