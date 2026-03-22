import { render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import CodeEditorPane from "./CodeEditorPane";

vi.mock("@monaco-editor/react", () => ({
  default: () => <div data-testid="mock-monaco-editor" />,
}));

const ORIGINAL_USER_AGENT = window.navigator.userAgent;

describe("CodeEditorPane", () => {
  beforeEach(() => {
    Object.defineProperty(window.navigator, "userAgent", {
      configurable: true,
      value: "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 Chrome/123.0.0.0 Safari/537.36",
    });
  });

  afterEach(() => {
    Object.defineProperty(window.navigator, "userAgent", {
      configurable: true,
      value: ORIGINAL_USER_AGENT,
    });
  });

  it("shows readable fallback content before monaco mounts", () => {
    render(
      <CodeEditorPane
        filePath="Cargo.toml"
        content={"[workspace]\nmembers = [\"anchor\"]\n"}
        isLoading={false}
        error={null}
      />
    );

    expect(screen.getByTestId("mock-monaco-editor")).toBeInTheDocument();
    expect(screen.getByLabelText(/code content/i)).toHaveTextContent("members = [\"anchor\"]");
  });
});
