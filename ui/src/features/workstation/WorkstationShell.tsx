import { Blocks, Files, GitBranch, Search, Settings } from "lucide-react";

import ActivityConsole from "./ActivityConsole";
import ChecklistPanel from "./ChecklistPanel";
import CodeEditorPane from "./CodeEditorPane";
import GraphLens from "./GraphLens";
import ProjectExplorer from "./ProjectExplorer";
import ReviewQueue from "./ReviewQueue";
import SecurityOverviewPanel from "./SecurityOverviewPanel";
import ToolbenchPanel from "./ToolbenchPanel";
import useSessionState from "./useSessionState";

type WorkstationShellProps = {
  sessionId: string;
};

const ACTIVITY_ITEMS = [
  { id: "files", label: "Explorer", icon: Files },
  { id: "search", label: "Search", icon: Search },
  { id: "source-control", label: "Source Control", icon: GitBranch },
  { id: "extensions", label: "Extensions", icon: Blocks },
] as const;

function fileTabLabel(path: string | null): string {
  if (!path) {
    return "welcome.md";
  }

  const segments = path.split("/");
  return segments[segments.length - 1] ?? path;
}

function WorkstationShell({ sessionId }: WorkstationShellProps): JSX.Element {
  const {
    projectTree,
    selectedFilePath,
    fileContent,
    consoleEntries,
    treeLoading,
    fileLoading,
    treeError,
    fileError,
    selectFile,
  } = useSessionState(sessionId);

  return (
    <div className="desktop-app-shell workstation-shell vscode-shell">
      <header className="vscode-titlebar">
        <div className="vscode-title-left">
          <div className="vscode-dot-row" aria-hidden>
            <span className="vscode-dot dot-close" />
            <span className="vscode-dot dot-minimize" />
            <span className="vscode-dot dot-expand" />
          </div>
          <p>Audit Agent</p>
        </div>
        <div className="vscode-title-center">audit-agent - {sessionId}</div>
        <div className="vscode-title-right">
          <button type="button" className="vscode-title-action">
            Split Editor
          </button>
        </div>
      </header>

      <main className="vscode-workbench">
        <aside className="vscode-activity-bar" aria-label="Activity Bar">
          <div className="vscode-activity-items">
            {ACTIVITY_ITEMS.map((item, index) => {
              const Icon = item.icon;
              const active = index === 0;
              return (
                <button
                  key={item.id}
                  type="button"
                  className={`vscode-activity-button${active ? " active" : ""}`}
                  aria-label={item.label}
                >
                  <Icon size={18} />
                </button>
              );
            })}
          </div>
          <button type="button" className="vscode-activity-button" aria-label="Settings">
            <Settings size={18} />
          </button>
        </aside>

        <ProjectExplorer
          sessionId={sessionId}
          nodes={projectTree}
          selectedFilePath={selectedFilePath}
          onSelectFile={selectFile}
          isLoading={treeLoading}
          error={treeError}
        />

        <section className="vscode-editor-column">
          <div className="vscode-editor-tabs" role="tablist" aria-label="Open files">
            <button type="button" className="vscode-editor-tab active" role="tab" aria-selected="true">
              {fileTabLabel(selectedFilePath)}
            </button>
          </div>
          <div className="vscode-editor-stack">
            <CodeEditorPane
              filePath={selectedFilePath}
              content={fileContent}
              isLoading={fileLoading}
              error={fileError}
            />
            <GraphLens sessionId={sessionId} />
          </div>
        </section>

        <aside className="vscode-right-column">
          <SecurityOverviewPanel sessionId={sessionId} />
          <ChecklistPanel sessionId={sessionId} />
          <ToolbenchPanel
            sessionId={sessionId}
            selection={
              selectedFilePath
                ? { kind: "file", id: selectedFilePath }
                : { kind: "session", id: sessionId }
            }
          />
          <ReviewQueue sessionId={sessionId} />
        </aside>
      </main>

      <ActivityConsole entries={consoleEntries} />
    </div>
  );
}

export default WorkstationShell;
