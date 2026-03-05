import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  chooseSavePath,
  confirmWorkspace,
  detectWorkspace,
  downloadOutput,
  getAuditManifest,
  isTauriRuntime,
  resolveSource,
} from "./commands";

describe("ipc commands", () => {
  beforeEach(() => {
    delete (window as typeof window & { __TAURI__?: unknown }).__TAURI__;
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("reports non-tauri runtime when invoke bridge is missing", () => {
    expect(isTauriRuntime()).toBe(false);
  });

  it("falls back to mock confirmWorkspace response in browser mode", async () => {
    const response = await confirmWorkspace({
      confirmed: true,
      ambiguousCrates: { "bridge-adapter": true },
    });

    expect(response.auditId).toContain("audit-");
  });

  it("uses tauri invoke bridge when available", async () => {
    const invoke = vi.fn(async () => ({ auditId: "audit-from-tauri" }));
    (window as any).__TAURI__ = {
      core: { invoke },
    };

    const response = await confirmWorkspace({
      confirmed: true,
      ambiguousCrates: { "bridge-adapter": false },
    });

    expect(isTauriRuntime()).toBe(true);
    expect(invoke).toHaveBeenCalledWith("confirm_workspace", {
      decisions: { confirmed: true, ambiguousCrates: { "bridge-adapter": false } },
    });
    expect(response.auditId).toBe("audit-from-tauri");
  });

  it("returns download destination through fallback in browser mode", async () => {
    const response = await downloadOutput(
      "audit-20260305-a1b2c3d4",
      "findings_json",
      "/tmp/findings.json"
    );
    expect(response.dest).toBe("/tmp/findings.json");
  });

  it("falls back for resolveSource and detectWorkspace in browser mode", async () => {
    const source = await resolveSource({
      kind: "git",
      value: "https://github.com/org/repo",
      commitOrRef: "a1b2c3d4ef5678",
    });
    expect(source.commitHash).toBe("a1b2c3d4ef5678");
    expect(source.branchResolutionBanner).toContain("Resolved to SHA");

    const workspace = await detectWorkspace();
    expect(workspace.crateCount).toBe(3);
    expect(workspace.frameworks.length).toBeGreaterThan(0);
  });

  it("returns a fallback audit manifest in browser mode", async () => {
    const manifest = await getAuditManifest();
    expect(manifest.auditId).toBe("audit-20260305-a1b2c3d4");
    expect(manifest.riskScore).toBe(65);
  });

  it("uses save dialog when available", async () => {
    const save = vi.fn(async () => "/tmp/chosen.json");
    (window as any).__TAURI__ = {
      dialog: { save },
    };

    const selected = await chooseSavePath("findings.json");
    expect(selected).toBe("/tmp/chosen.json");
    expect(save).toHaveBeenCalledWith({ defaultPath: "findings.json" });
  });
});
