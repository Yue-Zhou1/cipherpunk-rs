import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, vi } from "vitest";

import App from "./App";

afterEach(() => {
  delete (window as typeof window & { __TAURI__?: unknown }).__TAURI__;
  vi.restoreAllMocks();
});

describe("App layout shell", () => {
  it("renders desktop chrome with six-step indicator", () => {
    render(<App />);

    expect(screen.getByRole("banner")).toBeInTheDocument();
    expect(screen.getByText("Audit Agent")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /help/i })).toBeInTheDocument();

    const stepButtons = screen.getAllByRole("button", { name: /step \d/i });
    expect(stepButtons).toHaveLength(6);
    expect(screen.getByRole("button", { name: /step 1/i })).toHaveAttribute(
      "aria-current",
      "step"
    );
  });

  it("shows navigation footer for steps 1-4", () => {
    render(<App />);

    expect(screen.getByRole("contentinfo")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /next step/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /back/i })).toBeInTheDocument();
  });

  it("hides navigation footer for execution and result steps", () => {
    render(<App />);

    fireEvent.click(screen.getByRole("button", { name: /step 5/i }));
    expect(screen.queryByRole("contentinfo")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /step 6/i }));
    expect(screen.queryByRole("contentinfo")).not.toBeInTheDocument();
  });

  it("switches source form fields when source tabs change", () => {
    render(<App />);

    expect(screen.getByLabelText(/repository url/i)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("tab", { name: /local path/i }));

    expect(screen.getByLabelText(/workspace path/i)).toBeInTheDocument();
    expect(screen.queryByLabelText(/repository url/i)).not.toBeInTheDocument();
  });

  it("updates ambiguous crate decision from workspace actions", () => {
    render(<App />);

    fireEvent.click(screen.getByRole("button", { name: /step 4/i }));

    const row = screen.getByText("bridge-adapter").closest("tr");
    expect(row).not.toBeNull();

    fireEvent.click(within(row as HTMLTableRowElement).getByRole("button", { name: /include/i }));
    expect(within(row as HTMLTableRowElement).getByText(/in scope/i)).toBeInTheDocument();
  });

  it("syncs finding detail when a finding is selected in results view", () => {
    render(<App />);

    fireEvent.click(screen.getByRole("button", { name: /step 6/i }));
    fireEvent.click(screen.getByRole("button", { name: /nonce reuse risk/i }));

    expect(screen.getByText(/nonce reuse in signing function/i)).toBeInTheDocument();
    expect(screen.getByText(/framework:\s*rust crypto/i)).toBeInTheDocument();
  });

  it("disables next step when required source fields are invalid", () => {
    render(<App />);

    const nextButton = screen.getByRole("button", { name: /next step/i });
    expect(nextButton).toBeEnabled();

    fireEvent.change(screen.getByLabelText(/repository url/i), {
      target: { value: "" },
    });
    expect(nextButton).toBeDisabled();

    fireEvent.change(screen.getByLabelText(/repository url/i), {
      target: { value: "https://github.com/org/repo" },
    });
    expect(nextButton).toBeEnabled();
  });

  it("starts execution from workspace confirmation", async () => {
    render(<App />);

    fireEvent.click(screen.getByRole("button", { name: /step 4/i }));
    fireEvent.click(screen.getByRole("button", { name: /confirm and start audit/i }));

    expect(await screen.findByText(/audit running - circomlib/i)).toBeInTheDocument();
    expect(screen.queryByRole("contentinfo")).not.toBeInTheDocument();
  });

  it("requires valid configuration before progressing from step 2", () => {
    render(<App />);

    fireEvent.click(screen.getByRole("button", { name: /next step/i }));
    expect(screen.getByRole("heading", { level: 2, name: /audit configuration/i })).toBeInTheDocument();

    const nextButton = screen.getByRole("button", { name: /next step/i });
    expect(nextButton).toBeEnabled();

    fireEvent.change(screen.getByLabelText(/target crates/i), { target: { value: "" } });
    expect(nextButton).toBeDisabled();

    fireEvent.change(screen.getByLabelText(/target crates/i), { target: { value: "crate-a" } });
    expect(nextButton).toBeEnabled();
  });

  it("exports audit yaml from workspace confirmation", async () => {
    vi.spyOn(window, "prompt").mockReturnValue("/tmp/audit.yaml");

    render(<App />);

    fireEvent.click(screen.getByRole("button", { name: /step 4/i }));
    fireEvent.click(screen.getByRole("button", { name: /export audit\.yaml/i }));

    expect(await screen.findByText(/audit\.yaml exported/i)).toBeInTheDocument();
  });

  it("supports optional-input api key visibility toggle", () => {
    render(<App />);

    fireEvent.click(screen.getByRole("button", { name: /step 3/i }));

    const apiKeyInput = screen.getByLabelText(/api key/i) as HTMLInputElement;
    expect(apiKeyInput.type).toBe("password");

    fireEvent.click(screen.getByRole("button", { name: /show key/i }));
    expect(apiKeyInput.type).toBe("text");

    fireEvent.click(screen.getByRole("button", { name: /hide key/i }));
    expect(apiKeyInput.type).toBe("password");
  });

  it("shows live execution controls with collapsible logs", () => {
    render(<App />);

    fireEvent.click(screen.getByRole("button", { name: /step 5/i }));
    expect(screen.getByText(/audit running - circomlib/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /hide logs/i })).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /hide logs/i }));
    expect(screen.queryByRole("log", { name: /live logs/i })).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /show logs/i }));
    expect(screen.getByRole("log", { name: /live logs/i })).toBeInTheDocument();
  });

  it("renders results filters and six download actions", () => {
    render(<App />);

    fireEvent.click(screen.getByRole("button", { name: /step 6/i }));

    const severityFilter = screen.getByLabelText(/severity filter/i);
    fireEvent.change(severityFilter, { target: { value: "High" } });

    expect(screen.getByText(/showing 1 finding/i)).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: /download/i })).toHaveLength(6);
  });

  it("shows cdg for halo2 and trace viewer for distributed findings", () => {
    render(<App />);

    fireEvent.click(screen.getByRole("button", { name: /step 6/i }));
    expect(screen.getByText(/cdg view/i)).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /safety violation under partition/i }));
    expect(screen.getByText(/trace viewer/i)).toBeInTheDocument();
  });

  it("does not show branch-resolution warning on archive source tab", () => {
    render(<App />);

    fireEvent.click(screen.getByRole("tab", { name: /upload archive/i }));
    expect(screen.queryByText(/resolved to sha/i)).not.toBeInTheDocument();
  });

  it("renders workspace frameworks and warnings from IPC state", async () => {
    const invoke = vi.fn(async (command: string) => {
      if (command === "resolve_source") {
        return {
          commitHash: "5f6e7d8c9b",
          branchResolutionBanner: "Resolved to SHA 5f6e7d - audit is pinned to this commit",
          warnings: ["Branch release resolved to 5f6e7d8c9b"],
        };
      }

      if (command === "detect_workspace") {
        return {
          crateCount: 2,
          crates: [
            { name: "rollup-core", status: "in_scope" },
            { name: "bridge-adapter", status: "ambiguous" },
          ],
          frameworks: ["SP1", "RISC0"],
          warnings: ["LLM key missing. Degraded features: prose polish"],
          buildMatrix: [{ variant: "default", features: "default", estTime: "~20 min" }],
        };
      }

      return { auditId: "audit-from-tauri" };
    });

    (window as any).__TAURI__ = {
      core: { invoke },
    };

    render(<App />);

    fireEvent.click(screen.getByRole("button", { name: /next step/i }));
    fireEvent.click(screen.getByRole("button", { name: /next step/i }));
    fireEvent.click(screen.getByRole("button", { name: /next step/i }));

    expect(await screen.findByText("SP1")).toBeInTheDocument();
    expect(screen.getByText("RISC0")).toBeInTheDocument();
    expect(screen.getByText(/llm key missing/i)).toBeInTheDocument();
    expect(screen.queryByText("Circom")).not.toBeInTheDocument();
  });

  it("updates execution panel from tauri event stream payloads", async () => {
    type ExecutionEventListener = (event: { payload: unknown }) => void;
    let listener: ExecutionEventListener | undefined;
    const unlisten = vi.fn();
    const invoke = vi.fn(async () => ({ auditId: "audit-20260305-a1b2c3d4" }));

    (window as any).__TAURI__ = {
      core: { invoke },
      event: {
        listen: vi.fn(async (_name: string, cb: ExecutionEventListener) => {
          listener = cb;
          return unlisten;
        }),
      },
    };

    render(<App />);

    fireEvent.click(screen.getByRole("button", { name: /step 5/i }));
    expect(screen.getByText("Findings (live)")).toBeInTheDocument();

    if (!listener) {
      throw new Error("execution listener was not registered");
    }

    const emitUpdate: ExecutionEventListener = listener;
    act(() => {
      emitUpdate({
        payload: {
          auditId: "audit-20260305-a1b2c3d4",
          nodes: [
            { name: "Intake", channel: "intake", status: "done" },
            { name: "Rule Eval", channel: "rules", status: "running" },
          ],
          counts: { critical: 1, high: 2, medium: 3, low: 0, observation: 0 },
          logs: [{ timestamp: "14:00:00", channel: "rules", message: "rule engine running" }],
          latestFinding: "F-CR-111 Critical - signature forgery path",
        },
      });
    });

    expect(
      await screen.findByText(
        (_content, element) =>
          element?.tagName.toLowerCase() === "li" &&
          /Critical:\s*1/.test(element.textContent ?? "")
      )
    ).toBeInTheDocument();
    expect(screen.getByText(/signature forgery path/i)).toBeInTheDocument();
  });
});
