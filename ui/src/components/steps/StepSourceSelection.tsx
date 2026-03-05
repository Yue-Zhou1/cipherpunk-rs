import { AlertTriangle, FolderSearch, GitBranch, Upload } from "lucide-react";

import { SOURCE_TAB_LABELS } from "../../data/mockData";
import type { SourceMode } from "../../types";

type StepSourceSelectionProps = {
  mode: SourceMode;
  onModeChange: (mode: SourceMode) => void;
  gitUrl: string;
  gitRef: string;
  localPath: string;
  localCommit: string;
  archiveFileName: string;
  branchResolutionBanner: string | null;
  onGitUrlChange: (value: string) => void;
  onGitRefChange: (value: string) => void;
  onLocalPathChange: (value: string) => void;
  onArchiveSelect: () => void;
};

function StepSourceSelection({
  mode,
  onModeChange,
  gitUrl,
  gitRef,
  localPath,
  localCommit,
  archiveFileName,
  branchResolutionBanner,
  onGitUrlChange,
  onGitRefChange,
  onLocalPathChange,
  onArchiveSelect,
}: StepSourceSelectionProps): JSX.Element {
  return (
    <section className="step-card">
      <h2>Select audit source</h2>
      <div className="pill-tabs" role="tablist" aria-label="Source mode">
        {(Object.keys(SOURCE_TAB_LABELS) as SourceMode[]).map((tabMode) => (
          <button
            key={tabMode}
            type="button"
            role="tab"
            aria-selected={mode === tabMode}
            className={`pill-tab ${mode === tabMode ? "active" : ""}`}
            onClick={() => onModeChange(tabMode)}
          >
            {SOURCE_TAB_LABELS[tabMode]}
          </button>
        ))}
      </div>

      {mode === "git" ? (
        <GitSourceForm
          gitUrl={gitUrl}
          gitRef={gitRef}
          onGitUrlChange={onGitUrlChange}
          onGitRefChange={onGitRefChange}
        />
      ) : null}
      {mode === "local" ? (
        <LocalSourceForm
          localPath={localPath}
          localCommit={localCommit}
          onLocalPathChange={onLocalPathChange}
        />
      ) : null}
      {mode === "archive" ? (
        <ArchiveSourceForm archiveFileName={archiveFileName} onArchiveSelect={onArchiveSelect} />
      ) : null}

      {branchResolutionBanner ? (
        <div className="banner banner-warning">
          <AlertTriangle size={16} aria-hidden="true" />
          <span>{branchResolutionBanner}</span>
        </div>
      ) : null}
    </section>
  );
}

type GitSourceFormProps = {
  gitUrl: string;
  gitRef: string;
  onGitUrlChange: (value: string) => void;
  onGitRefChange: (value: string) => void;
};

function GitSourceForm({
  gitUrl,
  gitRef,
  onGitUrlChange,
  onGitRefChange,
}: GitSourceFormProps): JSX.Element {
  return (
    <div className="form-grid">
      <label>
        Repository URL
        <input type="text" value={gitUrl} onChange={(event) => onGitUrlChange(event.target.value)} />
      </label>
      <label>
        Commit SHA / Ref
        <input type="text" value={gitRef} onChange={(event) => onGitRefChange(event.target.value)} />
      </label>
    </div>
  );
}

type LocalSourceFormProps = {
  localPath: string;
  localCommit: string;
  onLocalPathChange: (value: string) => void;
};

function LocalSourceForm({ localPath, localCommit, onLocalPathChange }: LocalSourceFormProps): JSX.Element {
  return (
    <div className="form-grid">
      <label>
        Workspace Path
        <input
          type="text"
          value={localPath}
          onChange={(event) => onLocalPathChange(event.target.value)}
        />
      </label>
      <label>
        Detected Commit SHA
        <input type="text" value={localCommit} readOnly />
      </label>
      <div className="inline-panel" role="status" aria-label="Local source notes">
        <FolderSearch size={16} aria-hidden="true" />
        <span>Local workspace metadata scanned and mapped to current branch.</span>
      </div>
    </div>
  );
}

type ArchiveSourceFormProps = {
  archiveFileName: string;
  onArchiveSelect: () => void;
};

function ArchiveSourceForm({ archiveFileName, onArchiveSelect }: ArchiveSourceFormProps): JSX.Element {
  return (
    <div className="split-grid">
      <article className="panel">
        <h3>
          <Upload size={16} aria-hidden="true" />
          Upload Source Archive
        </h3>
        <button type="button" className="dropzone" onClick={onArchiveSelect}>
          {archiveFileName ? `Selected: ${archiveFileName}` : "Drop .tar.gz/.zip or click to browse"}
        </button>
      </article>
      <article className="panel">
        <h3>
          <GitBranch size={16} aria-hidden="true" />
          Archive Resolution
        </h3>
        <p className="muted-text">Archive is unpacked into a clean workspace and pinned to synthetic commit hash.</p>
      </article>
    </div>
  );
}

export default StepSourceSelection;
