import type { ProjectTreeNode } from "../../ipc/commands";

type ProjectExplorerProps = {
  sessionId: string;
  nodes: ProjectTreeNode[];
  selectedFilePath: string | null;
  onSelectFile: (path: string) => void;
  isLoading: boolean;
  error: string | null;
};

function TreeNodes({
  nodes,
  selectedFilePath,
  onSelectFile,
}: {
  nodes: ProjectTreeNode[];
  selectedFilePath: string | null;
  onSelectFile: (path: string) => void;
}): JSX.Element {
  return (
    <ul className="workstation-tree-list">
      {nodes.map((node) => {
        if (node.kind === "directory") {
          const children = node.children ?? [];

          return (
            <li key={node.path} className="workstation-tree-node workstation-tree-directory">
              <details open>
                <summary>{node.name}</summary>
                {children.length > 0 ? (
                  <TreeNodes
                    nodes={children}
                    selectedFilePath={selectedFilePath}
                    onSelectFile={onSelectFile}
                  />
                ) : null}
              </details>
            </li>
          );
        }

        const active = node.path === selectedFilePath;
        return (
          <li key={node.path} className="workstation-tree-node workstation-tree-file-row">
            <button
              type="button"
              className={`workstation-tree-file${active ? " active" : ""}`}
              onClick={() => onSelectFile(node.path)}
            >
              {node.name}
            </button>
          </li>
        );
      })}
    </ul>
  );
}

function ProjectExplorer({
  sessionId,
  nodes,
  selectedFilePath,
  onSelectFile,
  isLoading,
  error,
}: ProjectExplorerProps): JSX.Element {
  return (
    <section className="panel workstation-explorer" aria-label="Project Explorer">
      <div className="workstation-panel-head">
        <p className="workstation-panel-eyebrow">Explorer</p>
        <h2>Project Explorer</h2>
      </div>
      <p className="muted-text">Session {sessionId}</p>
      <label className="workstation-input-label">
        <span>Filter files</span>
        <input type="text" value="" readOnly aria-label="Filter files" />
      </label>

      {isLoading ? <p className="muted-text">Loading project tree...</p> : null}
      {error ? <p className="banner banner-error">{error}</p> : null}
      {!isLoading && !error && nodes.length === 0 ? (
        <p className="muted-text">No files are available in this session.</p>
      ) : null}

      {!isLoading && !error && nodes.length > 0 ? (
        <TreeNodes
          nodes={nodes}
          selectedFilePath={selectedFilePath}
          onSelectFile={onSelectFile}
        />
      ) : null}
    </section>
  );
}

export default ProjectExplorer;
