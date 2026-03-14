import { useEffect, useMemo, useState } from "react";

import {
  loadToolbenchContext,
  type ToolbenchContextResponse,
  type ToolbenchSelection,
} from "../../ipc/commands";

type ToolbenchPanelProps = {
  sessionId: string;
  selection?: ToolbenchSelection;
};

const DEFAULT_SELECTION: ToolbenchSelection = { kind: "session", id: "session" };

function ToolbenchPanel({
  sessionId,
  selection,
}: ToolbenchPanelProps): JSX.Element {
  const resolvedSelection = useMemo<ToolbenchSelection>(
    () => selection ?? DEFAULT_SELECTION,
    [selection?.kind, selection?.id]
  );
  const [context, setContext] = useState<ToolbenchContextResponse | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setIsLoading(true);
    setError(null);

    void loadToolbenchContext(sessionId, resolvedSelection)
      .then((response) => {
        if (!cancelled) {
          setContext(response);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setError("Unable to load toolbench recommendations.");
          setContext(null);
        }
      })
      .finally(() => {
        if (!cancelled) {
          setIsLoading(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [resolvedSelection.id, resolvedSelection.kind, sessionId]);

  return (
    <section className="panel workstation-toolbench" aria-label="Toolbench">
      <div className="workstation-panel-head">
        <p className="workstation-panel-eyebrow">Targeted Tools</p>
        <h2>Toolbench</h2>
      </div>
      <p className="muted-text">
        Session {sessionId} · {resolvedSelection.kind}:{resolvedSelection.id}
      </p>

      {isLoading ? <p className="muted-text">Loading tool recommendations...</p> : null}
      {error ? <p className="banner banner-error">{error}</p> : null}

      {!isLoading && !error && context ? (
        <>
          <section className="toolbench-section" aria-label="Recommended tools">
            <h3>Recommended Tools</h3>
            <div className="workstation-tool-list" role="list" aria-label="Tool actions">
              {context.recommendedTools.map((tool) => (
                <button
                  key={tool.toolId}
                  type="button"
                  className="inline-action workstation-tool-action"
                  role="listitem"
                  title={tool.rationale}
                >
                  {tool.toolId}
                </button>
              ))}
            </div>
          </section>

          <section className="toolbench-section" aria-label="Domain checklists">
            <h3>Domain Checklists</h3>
            <ul className="toolbench-list">
              {context.domains.map((domain) => (
                <li key={domain.id}>
                  <strong>{domain.id}</strong>
                  <p className="muted-text">{domain.rationale}</p>
                </li>
              ))}
            </ul>
          </section>

          <section className="toolbench-section" aria-label="Rationale notes">
            <h3>Rationale</h3>
            <ul className="toolbench-list">
              {context.overviewNotes.map((note) => (
                <li key={note}>{note}</li>
              ))}
            </ul>
          </section>

          <section className="toolbench-section" aria-label="Similar cases">
            <h3>Similar Cases</h3>
            <ul className="toolbench-list">
              {context.similarCases.map((item) => (
                <li key={item.id}>
                  <strong>{item.title}</strong>
                  <p className="muted-text">{item.summary}</p>
                </li>
              ))}
            </ul>
          </section>
        </>
      ) : null}
    </section>
  );
}

export default ToolbenchPanel;
