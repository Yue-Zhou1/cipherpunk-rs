# Workstation View Switcher Layout Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace the broken stacked-components web layout with a viewport-locked three-view workstation driven by the activity bar, eliminating all scrollbars and enabling a full-window interactive DAG as the default view.

**Architecture:** The shell locks to `height: 100vh; overflow: hidden`. The activity bar becomes a view switcher with three views: Graph (full-window DAG, default), Editor (3-col with pinned console), and Info (tab-switched panels). All view state lives in `WorkstationShell` as `activeView` + `activeInfoTab`. The `ExplorerProvider` stays always mounted (inside the Graph view) so graph state is never lost on tab switch.

**Tech Stack:** React 18, TypeScript, CSS (no new dependencies), Vitest + Testing Library

---

## Context: How the current code is structured

- [`ui/src/features/workstation/WorkstationShell.tsx`](ui/src/features/workstation/WorkstationShell.tsx) — top-level shell. Has two paths: `useSplitLayout` (desktop/Allotment) and web mode. **We only touch the web mode path** (`webMode === true` branch, lines 271–307). The desktop Allotment path (lines 202–269) is **not changed**.
- [`ui/src/styles.css`](ui/src/styles.css) — all layout CSS. Workstation styles are around lines 776–1040, explorer styles around 1918+.
- [`ui/src/features/workstation/CodebaseExplorer/index.tsx`](ui/src/features/workstation/CodebaseExplorer/index.tsx) — renders `ExplorerProvider` > `ExplorerLayout` > `ExplorerToolbar` + canvas. No prop changes needed.
- [`ui/src/features/workstation/WorkstationShell.test.tsx`](ui/src/features/workstation/WorkstationShell.test.tsx) — existing tests must keep passing; we add new ones for view switching.

## What "web mode" means

`webMode = getTransport().kind === "http"`. In tests, `transportKind` is set to `"http"` to exercise this path. All new view-switcher behavior is **web mode only**.

---

## Task 1: Add `activeView` state and update activity bar buttons

**Files:**
- Modify: `ui/src/features/workstation/WorkstationShell.tsx`

This task wires up the state and makes activity bar buttons switch views. No layout change yet — we just add state and update button click handlers. The three views are `"graph" | "editor" | "info"`.

**Step 1: Add the import for the new icon**

At the top of `WorkstationShell.tsx`, `Blocks` and `Files` are already imported from `lucide-react`. Add `Network` to the import (this will be the graph view icon):

```tsx
import { Blocks, Files, GitBranch, Network, Search, Settings } from "lucide-react";
```

**Step 2: Add `activeView` state inside `WorkstationShell`**

After the existing `useState` declarations (around line 121), add:

```tsx
const [activeView, setActiveView] = useState<"graph" | "editor" | "info">("graph");
const [activeInfoTab, setActiveInfoTab] = useState<"overview" | "checklist" | "audit" | "toolbench" | "review">("overview");
```

**Step 3: Replace `ACTIVITY_ITEMS` constant with typed view config**

Remove the `ACTIVITY_ITEMS` const (lines 24–29) and replace with:

```tsx
const VIEW_ITEMS = [
  { id: "graph" as const, label: "Graph", icon: Network },
  { id: "editor" as const, label: "Editor", icon: Files },
  { id: "info" as const, label: "Info", icon: Blocks },
] satisfies { id: "graph" | "editor" | "info"; label: string; icon: React.ElementType }[];
```

**Step 4: Update the activity bar JSX**

Replace the activity bar's `{ACTIVITY_ITEMS.map(...)}` block (lines 183–195) with:

```tsx
{VIEW_ITEMS.map((item) => {
  const Icon = item.icon;
  return (
    <button
      key={item.id}
      type="button"
      className={`vscode-activity-button${activeView === item.id ? " active" : ""}`}
      aria-label={item.label}
      onClick={() => setActiveView(item.id)}
    >
      <Icon size={18} />
    </button>
  );
})}
```

**Step 5: Write failing tests**

In `ui/src/features/workstation/WorkstationShell.test.tsx`, add a new `describe` block at the bottom:

```tsx
describe("WorkstationShell web mode view switching", () => {
  beforeEach(() => {
    transportKind = "http";
    selectFileSpy.mockClear();
  });

  it("defaults to graph view showing codebase explorer", () => {
    render(<WorkstationShell sessionId="sess-1" />);
    expect(screen.getByTestId("codebase-explorer-state")).toBeInTheDocument();
  });

  it("switches to editor view when Editor button is clicked", () => {
    render(<WorkstationShell sessionId="sess-1" />);
    fireEvent.click(screen.getByRole("button", { name: /^editor$/i }));
    expect(screen.getByRole("heading", { name: /code editor/i })).toBeInTheDocument();
  });

  it("switches to info view when Info button is clicked", () => {
    render(<WorkstationShell sessionId="sess-1" />);
    fireEvent.click(screen.getByRole("button", { name: /^info$/i }));
    expect(screen.getByText(/security overview/i)).toBeInTheDocument();
  });
});
```

**Step 6: Run tests — expect failures**

```bash
cd ui && npx vitest run src/features/workstation/WorkstationShell.test.tsx
```

Expected: new tests fail (views not yet rendered conditionally), existing tests still pass.

**Step 7: Commit**

```bash
git add ui/src/features/workstation/WorkstationShell.tsx ui/src/features/workstation/WorkstationShell.test.tsx
git commit -m "feat(web-ui): add activeView state and view-switcher activity bar buttons"
```

---

## Task 2: Implement the three view layouts in JSX

**Files:**
- Modify: `ui/src/features/workstation/WorkstationShell.tsx`

Replace the web mode JSX block (the `webMode ? (...)` branch, currently lines 271–307) with the three-view conditional. The desktop Allotment path stays untouched.

**Step 1: Replace the web mode branch**

Find this block (the entire `webMode ? (` ternary inside the non-`useSplitLayout` branch):

```tsx
webMode ? (
  <div className="workstation-web-grid">
    <ProjectExplorer ... />
    <section className="vscode-editor-column">
      ...
      <div className="vscode-editor-stack">
        <CodeEditorPane ... />
        <CodebaseExplorer ... />
      </div>
    </section>
    <aside className="vscode-right-column workstation-web-sidebar">
      <SecurityOverviewPanel sessionId={sessionId} />
    </aside>
  </div>
) : (
  ... {/* desktop non-allotment fallback */}
```

Replace **only** the `webMode ? (...)` arm (keep the `: (...)` fallback for non-web/non-allotment) with:

```tsx
webMode ? (
  <>
    {/* Graph view */}
    <div
      className="view-panel view-graph"
      hidden={activeView !== "graph"}
      aria-hidden={activeView !== "graph"}
    >
      <CodebaseExplorer
        sessionId={sessionId}
        onNavigateToSource={handleNavigateToSource}
      />
    </div>

    {/* Editor view */}
    <div
      className="view-panel view-editor"
      hidden={activeView !== "editor"}
      aria-hidden={activeView !== "editor"}
    >
      <ProjectExplorer
        sessionId={sessionId}
        nodes={projectTree}
        selectedFilePath={selectedFilePath}
        onSelectFile={handleSelectFile}
        isLoading={treeLoading}
        error={treeError}
      />
      <section className="vscode-editor-column">
        <div className="vscode-editor-tabs" role="tablist" aria-label="Open files">
          <button type="button" className="vscode-editor-tab active" role="tab" aria-selected="true">
            {fileTabLabel(selectedFilePath)}
          </button>
        </div>
        <CodeEditorPane
          filePath={selectedFilePath}
          content={fileContent}
          isLoading={fileLoading}
          error={fileError}
          preferPlainText
          focusedRecordId={selectedReviewRecordId}
          focusedNodeCount={selectedGraphNodeIds.length}
          targetLine={targetLine}
        />
        <ActivityConsole sessionId={sessionId} entries={consoleEntries} />
      </section>
    </div>

    {/* Info view */}
    <div
      className="view-panel view-info"
      hidden={activeView !== "info"}
      aria-hidden={activeView !== "info"}
    >
      <nav className="info-tab-bar" role="tablist" aria-label="Info panels">
        {(
          [
            { id: "overview", label: "Security Overview" },
            { id: "checklist", label: "Checklist" },
            { id: "audit", label: "Audit Plan" },
            { id: "toolbench", label: "Toolbench" },
            { id: "review", label: "Review Queue" },
          ] as const
        ).map((tab) => (
          <button
            key={tab.id}
            type="button"
            role="tab"
            aria-selected={activeInfoTab === tab.id}
            className={`info-tab-button${activeInfoTab === tab.id ? " active" : ""}`}
            onClick={() => setActiveInfoTab(tab.id)}
          >
            {tab.label}
          </button>
        ))}
      </nav>
      <div className="info-panel-body">
        {activeInfoTab === "overview" && <SecurityOverviewPanel sessionId={sessionId} />}
        {activeInfoTab === "checklist" && <ChecklistPanel sessionId={sessionId} />}
        {activeInfoTab === "audit" && <AuditPlanPanel sessionId={sessionId} />}
        {activeInfoTab === "toolbench" && (
          <ToolbenchPanel
            sessionId={sessionId}
            selection={
              selectedFilePath
                ? { kind: "file", id: selectedFilePath }
                : { kind: "session", id: sessionId }
            }
          />
        )}
        {activeInfoTab === "review" && (
          <ReviewQueue
            sessionId={sessionId}
            selectedRecordId={selectedReviewRecordId}
            onSelectRecord={handleSelectReviewRecord}
          />
        )}
      </div>
    </div>
  </>
) : (
```

**Step 2: Remove the standalone `<ActivityConsole>` at the bottom**

The `<ActivityConsole>` is now inside the editor view. Remove the standalone render at line 366:

```tsx
// DELETE this line:
<ActivityConsole sessionId={sessionId} entries={consoleEntries} />
```

**Step 3: Run tests**

```bash
cd ui && npx vitest run src/features/workstation/WorkstationShell.test.tsx
```

Expected: The three new view-switching tests pass. Check that the existing tests still pass — note that the existing test `"renders code editor and graph side by side in http mode"` now needs to be updated since the graph is the default view (no "Graph" role tab anymore). Update that test:

```tsx
it("shows graph view by default in http mode", () => {
  transportKind = "http";
  render(<WorkstationShell sessionId="sess-1" />);
  expect(screen.getByTestId("codebase-explorer-state")).toBeInTheDocument();
});
```

And update `"navigates to source without losing graph in http mode"` — it clicks "Navigate from Graph" which triggers `handleNavigateToSource`. The graph is still mounted (just `hidden`). Test should still pass as-is since `hidden` doesn't remove from DOM.

Run all workstation tests:
```bash
cd ui && npx vitest run src/features/workstation/
```

Expected: All pass.

**Step 4: Commit**

```bash
git add ui/src/features/workstation/WorkstationShell.tsx ui/src/features/workstation/WorkstationShell.test.tsx
git commit -m "feat(web-ui): implement three-view layout (graph/editor/info) for web mode"
```

---

## Task 3: Fix viewport-locking CSS and view panel layout

**Files:**
- Modify: `ui/src/styles.css`

This is the CSS-only task. All layout bugs (page taller than viewport, scrollbars, console pushing height) are fixed here.

**Step 1: Lock the shell to the viewport**

Find `.vscode-shell` (around line 780):
```css
.vscode-shell {
  max-width: none;
  margin: 0;
  padding: 0;
  min-height: 100vh;
  background: #1e1e1e;
  color: #cccccc;
  font-family: "IBM Plex Sans", "Plus Jakarta Sans", "Segoe UI", sans-serif;
}
```

Change `min-height: 100vh` to `height: 100vh` and add `overflow: hidden`:
```css
.vscode-shell {
  max-width: none;
  margin: 0;
  padding: 0;
  height: 100vh;
  overflow: hidden;
  background: #1e1e1e;
  color: #cccccc;
  font-family: "IBM Plex Sans", "Plus Jakarta Sans", "Segoe UI", sans-serif;
}
```

**Step 2: Fix `.vscode-workbench` to fill remaining height without hardcoded calc**

Find `.vscode-workbench` (around line 866):
```css
.vscode-workbench {
  flex: 1;
  min-height: calc(100vh - 34px - 210px);
  display: grid;
  grid-template-columns: 48px 280px minmax(0, 1fr) 320px;
}
```

Change to:
```css
.vscode-workbench {
  flex: 1;
  min-height: 0;
  display: grid;
  grid-template-columns: 48px 280px minmax(0, 1fr) 320px;
}
```

Also fix the responsive overrides — search for all occurrences of `min-height: calc(100vh` in the file and change them to `min-height: 0`.

**Step 3: Add view panel base class and per-view styles**

After the `.workstation-web-grid` block (around line 964), add:

```css
/* --- View switcher panels (web mode) --- */

.view-panel {
  flex: 1;
  min-height: 0;
  overflow: hidden;
  display: flex;
  flex-direction: column;
}

.view-panel[hidden] {
  display: none;
}

/* Graph view: CodebaseExplorer fills 100% */
.view-graph {
  background: #0a0e17;
}

/* Editor view: sidebar + editor column side by side */
.view-editor {
  display: grid;
  grid-template-columns: 220px minmax(0, 1fr);
}

.view-editor .vscode-editor-column {
  display: flex;
  flex-direction: column;
  min-height: 0;
}

.view-editor .vscode-editor-column .code-editor-pane-root,
.view-editor .vscode-editor-column > section,
.view-editor .vscode-editor-column > div:not(.vscode-editor-tabs) {
  flex: 1;
  min-height: 0;
}

/* Info view: tab bar + scrollable content */
.view-info {
  background: #1e1e1e;
}

.info-tab-bar {
  display: flex;
  flex-shrink: 0;
  gap: 0;
  background: #2d2d2d;
  border-bottom: 1px solid #252526;
  overflow-x: auto;
}

.info-tab-button {
  height: 34px;
  padding: 0 16px;
  border: 0;
  background: transparent;
  color: #8b8b8b;
  font-size: 12px;
  cursor: pointer;
  white-space: nowrap;
  border-bottom: 2px solid transparent;
  transition: color 150ms ease-out, border-color 150ms ease-out;
}

.info-tab-button:hover {
  color: #d4d4d4;
  background: #2a2d2e;
}

.info-tab-button.active {
  color: #ffffff;
  border-bottom-color: #0078d4;
  background: #1e1e1e;
}

.info-panel-body {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  background: #252526;
}
```

**Step 4: Fix ActivityConsole — remove `min-height`, make it a pinned strip**

Find `.workstation-console` (around line 1660):
```css
.workstation-console {
  min-height: 210px;
  border-top: 1px solid #252526;
  background: #1e1e1e;
}
```

Change to:
```css
.workstation-console {
  height: 180px;
  flex-shrink: 0;
  border-top: 1px solid #252526;
  background: #1e1e1e;
  overflow-y: auto;
}
```

**Step 5: Verify in browser**

Start the web server:
```bash
cd /home/zhouy/personal_projects/cipherpunk-rs && bash scripts/start-web-http.sh
```

Open the browser. Verify:
- No vertical scrollbar on the page
- Graph view fills the full window by default
- Clicking Editor shows the 3-col layout with console pinned at bottom
- Clicking Info shows the tab bar with all 5 tabs

**Step 6: Commit**

```bash
git add ui/src/styles.css
git commit -m "fix(web-ui): lock shell to viewport height, add view panel layout CSS"
```

---

## Task 4: Fix `CodeEditorPane` height inside editor view

**Files:**
- Modify: `ui/src/styles.css`

The `CodeEditorPane` uses Monaco editor which needs an explicit pixel height on its container. After the previous task, the editor may not render (Monaco renders into a 0-height div if the container doesn't have a resolved height). This task fixes that.

**Step 1: Identify the editor pane root class**

Read `ui/src/features/workstation/CodeEditorPane.tsx` to find the root element's className:

```bash
grep -n "className" ui/src/features/workstation/CodeEditorPane.tsx | head -10
```

**Step 2: Add height: 100% to the editor pane container**

Find the CSS class for the CodeEditorPane root in `styles.css`. Add `height: 100%` if missing. If the class is `workstation-editor` or `.vscode-editor-column .workstation-editor`, ensure:

```css
.view-editor .vscode-editor-column .workstation-editor {
  flex: 1;
  min-height: 0;
  height: 100%;
}
```

Adjust the selector to match whatever class `CodeEditorPane.tsx` actually uses on its outermost `div`.

**Step 3: Visual check**

Reload the browser. Switch to Editor view. The Monaco editor should fill the available space above the console strip.

**Step 4: Commit**

```bash
git add ui/src/styles.css
git commit -m "fix(web-ui): ensure CodeEditorPane fills available height in editor view"
```

---

## Task 5: Update existing tests that break due to view switching

**Files:**
- Modify: `ui/src/features/workstation/WorkstationShell.test.tsx`

The original test `"renders explorer, editor, toolbench, and console panels"` runs in `tauri` mode (not web), so it uses the Allotment path — no change needed. But the http-mode test `"renders code editor and graph side by side in http mode"` references a tab that no longer exists. Fix it.

**Step 1: Check which tests fail**

```bash
cd ui && npx vitest run src/features/workstation/WorkstationShell.test.tsx
```

**Step 2: Update the outdated http-mode test**

The test on line 166 (`"renders code editor and graph side by side in http mode"`) expects:
```tsx
expect(screen.queryByRole("tab", { name: /^graph$/i })).not.toBeInTheDocument();
```

This was checking absence of a tab — it should still pass. But verify it doesn't accidentally fail. If it does, update the assertion to match the new DOM structure.

**Step 3: Run all UI tests**

```bash
cd ui && npx vitest run
```

Expected: All tests pass.

**Step 4: Commit**

```bash
git add ui/src/features/workstation/WorkstationShell.test.tsx
git commit -m "test(web-ui): update workstation tests to match three-view layout"
```

---

## Task 6: Final visual polish pass

**Files:**
- Modify: `ui/src/styles.css`

**Step 1: Verify active view indicator on activity bar**

The `.vscode-activity-button.active` already has `border-left-color: #0078d4`. Confirm it's visible for the current view.

**Step 2: Ensure `ProjectExplorer` panel scrolls internally**

The project tree can be long. The `ProjectExplorer` sidebar in the editor view must scroll internally. Find `.workstation-explorer` in `styles.css` and ensure it has `overflow-y: auto`.

**Step 3: Ensure Info panel body scrolls for tall panels**

The `.info-panel-body` already has `overflow-y: auto` from Task 3. Verify `SecurityOverviewPanel`, `ChecklistPanel`, etc. don't have their own `min-height` values that push past the container.

Search for `min-height` in the right-column panel CSS:
```bash
grep -n "min-height" ui/src/styles.css | grep -i "overview\|checklist\|audit\|toolbench\|review"
```

If any panel sets a `min-height` larger than the available space, override it inside `.info-panel-body > *`.

**Step 4: Remove orphaned `.workstation-web-grid` and `.workstation-web-sidebar` CSS**

Since we no longer use these classes, remove them from `styles.css` to keep things clean:
```css
/* DELETE these blocks: */
.workstation-web-grid { ... }
.workstation-web-grid .vscode-editor-column { ... }
.workstation-web-sidebar { ... }
```

Also remove `.vscode-editor-stack` if it's only used in web mode (the Allotment path has its own grid).

**Step 5: Full test run**

```bash
cd ui && npx vitest run
```

Expected: All tests pass, no regressions.

**Step 6: Final commit**

```bash
git add ui/src/styles.css
git commit -m "style(web-ui): polish view switcher, remove obsolete web-grid CSS"
```

---

## Summary of files changed

| File | Change |
|------|--------|
| `ui/src/features/workstation/WorkstationShell.tsx` | Add `activeView`/`activeInfoTab` state, view-switch activity bar, three-view JSX, move `ActivityConsole` into editor view |
| `ui/src/styles.css` | Lock shell to `100vh`, fix workbench height, add `.view-panel`/`.view-graph`/`.view-editor`/`.view-info`/`.info-tab-*` CSS, fix console height |
| `ui/src/features/workstation/WorkstationShell.test.tsx` | Add view-switching tests, update outdated http-mode test |

## Files NOT changed

- All panel components (`SecurityOverviewPanel`, `ChecklistPanel`, `AuditPlanPanel`, `ToolbenchPanel`, `ReviewQueue`, `ActivityConsole`)
- `CodebaseExplorer/` — no prop changes
- Desktop/Allotment layout path in `WorkstationShell.tsx`
- `App.tsx`, `WizardShell.tsx`, or any other non-workstation files
