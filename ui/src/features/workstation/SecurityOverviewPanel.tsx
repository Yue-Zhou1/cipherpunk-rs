import { useEffect, useState } from "react";

import {
  loadSecurityOverview,
  type SecurityOverviewResponse,
} from "../../ipc/commands";

type SecurityOverviewPanelProps = {
  sessionId: string;
};

function SecurityOverviewPanel({ sessionId }: SecurityOverviewPanelProps): JSX.Element {
  const [overview, setOverview] = useState<SecurityOverviewResponse | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setIsLoading(true);
    setError(null);

    void loadSecurityOverview(sessionId)
      .then((response) => {
        if (!cancelled) {
          setOverview(response);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setError("Unable to load security overview.");
          setOverview(null);
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
  }, [sessionId]);

  return (
    <section className="panel workstation-overview" aria-label="Security Overview">
      <div className="workstation-panel-head">
        <p className="workstation-panel-eyebrow">Project Overview</p>
        <h2>Security Overview</h2>
      </div>

      {isLoading ? <p className="muted-text">Loading overview...</p> : null}
      {error ? <p className="banner banner-error">{error}</p> : null}

      {!isLoading && !error && overview ? (
        <div className="overview-grid">
          <article>
            <h3>Assets</h3>
            <ul>
              {overview.assets.map((asset) => (
                <li key={asset}>{asset}</li>
              ))}
            </ul>
          </article>
          <article>
            <h3>Trust Boundaries</h3>
            <ul>
              {overview.trustBoundaries.map((boundary) => (
                <li key={boundary}>{boundary}</li>
              ))}
            </ul>
          </article>
          <article>
            <h3>Hotspots</h3>
            <ul>
              {overview.hotspots.map((hotspot) => (
                <li key={hotspot}>{hotspot}</li>
              ))}
            </ul>
          </article>
          <article>
            <h3>Review Notes</h3>
            <ul>
              {overview.reviewNotes.map((note) => (
                <li key={note}>{note}</li>
              ))}
            </ul>
          </article>
        </div>
      ) : null}
    </section>
  );
}

export default SecurityOverviewPanel;
