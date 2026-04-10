import { useCallback, useState, type ElementType } from "react";
import { Allotment } from "allotment";
import { Blocks, Files, Network, Settings } from "lucide-react";

import { Button } from "../../components/ui/button";
import type { ProjectTreeNode, ReviewQueueItem } from "../../ipc/commands";
import { getTransport } from "../../ipc/transport";
import "allotment/dist/style.css";
import ActivityConsole from "./ActivityConsole";
import AuditPlanPanel from "./AuditPlanPanel";
import ChecklistPanel from "./ChecklistPanel";
import CodeEditorPane from "./CodeEditorPane";
import CodebaseExplorer from "./CodebaseExplorer";
import ProjectExplorer from "./ProjectExplorer";
import ReviewQueue from "./ReviewQueue";
import SecurityOverviewPanel from "./SecurityOverviewPanel";
import ToolbenchPanel from "./ToolbenchPanel";
import useSessionState from "./useSessionState";

type WorkstationShellProps = {
  sessionId: string;
};

const VIEW_ITEMS = [
  { id: "graph" as const, label: "Graph", icon: Network },
  { id: "editor" as const, label: "Editor", icon: Files },
  { id: "info" as const, label: "Info", icon: Blocks },
] satisfies { id: "graph" | "editor" | "info"; label: string; icon: ElementType }[];

function fileTabLabel(path: string | null): string {
  if (!path) {
    return "welcome.md";
  }

  const segments = path.split("/");
  return segments[segments.length - 1] ?? path;
}

function collectFilePaths(nodes: ProjectTreeNode[]): string[] {
  const files: string[] = [];
  for (const node of nodes) {
    if (node.kind === "file") {
      files.push(node.path);
      continue;
    }
    files.push(...collectFilePaths(node.children ?? []));
  }
  return files;
}

function fileCandidatesFromNodeIds(nodeIds: string[]): string[] {
  const candidates: string[] = [];
  for (const nodeId of nodeIds) {
    if (nodeId.startsWith("file:")) {
      candidates.push(nodeId.slice("file:".length));
      continue;
    }
    if (nodeId.startsWith("symbol:")) {
      const rest = nodeId.slice("symbol:".length);
      const [path] = rest.split("::");
      if (path) {
        candidates.push(path);
      }
    }
  }
  return Array.from(new Set(candidates));
}

function resolveSelectedFilePath(
  nodeIds: string[] | undefined,
  treeNodes: ProjectTreeNode[]
): string | null {
  if (!nodeIds || nodeIds.length === 0) {
    return null;
  }

  const candidates = fileCandidatesFromNodeIds(nodeIds).map((value) =>
    value.replaceAll("\\", "/")
  );
  if (candidates.length === 0) {
    return null;
  }

  const available = collectFilePaths(treeNodes).map((value) => value.replaceAll("\\", "/"));
  for (const candidate of candidates) {
    if (available.includes(candidate)) {
      return candidate;
    }
    const bySuffix = available.find((path) => {
      const normalized = path.replaceAll("\\", "/");
      return (
        candidate === normalized || candidate.endsWith(`/${normalized}`)
      );
    });
    if (bySuffix) {
      return bySuffix;
    }
  }

  return null;
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
  const webMode = getTransport().kind === "http";
  const useSplitLayout =
    typeof navigator !== "undefined" &&
    !navigator.userAgent.toLowerCase().includes("jsdom") &&
    !webMode;
  const [selectedReviewRecordId, setSelectedReviewRecordId] = useState<string | null>(null);
  const [selectedGraphNodeIds, setSelectedGraphNodeIds] = useState<string[]>([]);
  const [targetLine, setTargetLine] = useState<number | null>(null);
  const [activeView, setActiveView] = useState<"graph" | "editor" | "info">("graph");
  const [activeInfoTab, setActiveInfoTab] = useState<"overview" | "checklist" | "audit" | "toolbench" | "review">("overview");

  const handleSelectFile = useCallback(
    (path: string) => {
      setTargetLine(null);
      selectFile(path);
    },
    [selectFile]
  );

  const handleNavigateToSource = useCallback(
    (filePath: string, line?: number) => {
      setTargetLine(typeof line === "number" ? line : null);
      selectFile(filePath);
    },
    [selectFile]
  );

  const handleSelectReviewRecord = useCallback(
    (item: ReviewQueueItem) => {
      setSelectedReviewRecordId(item.recordId);
      const nodeIds = item.irNodeIds ?? [];
      setSelectedGraphNodeIds(nodeIds);

      const matchedFilePath = resolveSelectedFilePath(nodeIds, projectTree);
      if (matchedFilePath) {
        handleSelectFile(matchedFilePath);
      }
    },
    [handleSelectFile, projectTree]
  );

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
          <Button type="button" variant="ghost" size="sm" className="vscode-title-action">
            Split Editor
          </Button>
        </div>
      </header>

      <main
        className={
          useSplitLayout
            ? "vscode-workbench workstation-resizable"
            : "vscode-workbench"
        }
      >
        <aside className="vscode-activity-bar" aria-label="Activity Bar">
          <div className="vscode-activity-items">
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
          </div>
          <button type="button" className="vscode-activity-button" aria-label="Settings">
            <Settings size={18} />
          </button>
        </aside>
        {useSplitLayout ? (
          <Allotment className="workstation-main-allotment" defaultSizes={[22, 53, 25]}>
            <Allotment.Pane minSize={180}>
              <ProjectExplorer
                sessionId={sessionId}
                nodes={projectTree}
                selectedFilePath={selectedFilePath}
                onSelectFile={handleSelectFile}
                isLoading={treeLoading}
                error={treeError}
              />
            </Allotment.Pane>

            <Allotment.Pane minSize={460}>
              <section className="vscode-editor-column">
                <div className="vscode-editor-tabs" role="tablist" aria-label="Open files">
                  <button type="button" className="vscode-editor-tab active" role="tab" aria-selected="true">
                    {fileTabLabel(selectedFilePath)}
                  </button>
                </div>
                <Allotment vertical className="workstation-editor-allotment" defaultSizes={[58, 42]}>
                  <Allotment.Pane minSize={180}>
                    <CodeEditorPane
                      filePath={selectedFilePath}
                      content={fileContent}
                      isLoading={fileLoading}
                      error={fileError}
                      preferPlainText={webMode}
                      focusedRecordId={selectedReviewRecordId}
                      focusedNodeCount={selectedGraphNodeIds.length}
                      targetLine={targetLine}
                    />
                  </Allotment.Pane>
                  <Allotment.Pane minSize={180}>
                    <CodebaseExplorer
                      sessionId={sessionId}
                      onNavigateToSource={handleNavigateToSource}
                    />
                  </Allotment.Pane>
                </Allotment>
              </section>
            </Allotment.Pane>

            <Allotment.Pane minSize={220}>
              <aside className="vscode-right-column">
                <SecurityOverviewPanel sessionId={sessionId} />
                {webMode ? null : (
                  <>
                    <ChecklistPanel sessionId={sessionId} />
                    <AuditPlanPanel sessionId={sessionId} />
                    <ToolbenchPanel
                      sessionId={sessionId}
                      selection={
                        selectedFilePath
                          ? { kind: "file", id: selectedFilePath }
                          : { kind: "session", id: sessionId }
                      }
                    />
                    <ReviewQueue
                      sessionId={sessionId}
                      selectedRecordId={selectedReviewRecordId}
                      onSelectRecord={handleSelectReviewRecord}
                    />
                  </>
                )}
              </aside>
            </Allotment.Pane>
          </Allotment>
        ) : (
          webMode ? (
            <>
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
                  {activeInfoTab === "overview" ? <SecurityOverviewPanel sessionId={sessionId} /> : null}
                  {activeInfoTab === "checklist" ? <ChecklistPanel sessionId={sessionId} /> : null}
                  {activeInfoTab === "audit" ? <AuditPlanPanel sessionId={sessionId} /> : null}
                  {activeInfoTab === "toolbench" ? (
                    <ToolbenchPanel
                      sessionId={sessionId}
                      selection={
                        selectedFilePath
                          ? { kind: "file", id: selectedFilePath }
                          : { kind: "session", id: sessionId }
                      }
                    />
                  ) : null}
                  {activeInfoTab === "review" ? (
                    <ReviewQueue
                      sessionId={sessionId}
                      selectedRecordId={selectedReviewRecordId}
                      onSelectRecord={handleSelectReviewRecord}
                    />
                  ) : null}
                </div>
              </div>
            </>
          ) : (
            <>
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
                <div className="vscode-editor-stack">
                  <CodeEditorPane
                    filePath={selectedFilePath}
                    content={fileContent}
                    isLoading={fileLoading}
                    error={fileError}
                    preferPlainText={webMode}
                    focusedRecordId={selectedReviewRecordId}
                    focusedNodeCount={selectedGraphNodeIds.length}
                    targetLine={targetLine}
                  />
                  <CodebaseExplorer
                    sessionId={sessionId}
                    onNavigateToSource={handleNavigateToSource}
                  />
                </div>
              </section>
              <aside className="vscode-right-column">
                <SecurityOverviewPanel sessionId={sessionId} />
                <>
                  <ChecklistPanel sessionId={sessionId} />
                  <AuditPlanPanel sessionId={sessionId} />
                  <ToolbenchPanel
                    sessionId={sessionId}
                    selection={
                      selectedFilePath
                        ? { kind: "file", id: selectedFilePath }
                        : { kind: "session", id: sessionId }
                    }
                  />
                  <ReviewQueue
                    sessionId={sessionId}
                    selectedRecordId={selectedReviewRecordId}
                    onSelectRecord={handleSelectReviewRecord}
                  />
                </>
              </aside>
            </>
          )
        )}
      </main>
      {!webMode ? <ActivityConsole sessionId={sessionId} entries={consoleEntries} /> : null}
    </div>
  );
}

export default WorkstationShell;
