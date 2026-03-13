import { useEffect, useState } from "react";

import {
  loadChecklistPlan,
  type ChecklistPlanResponse,
} from "../../ipc/commands";

type ChecklistPanelProps = {
  sessionId: string;
};

function ChecklistPanel({ sessionId }: ChecklistPanelProps): JSX.Element {
  const [plan, setPlan] = useState<ChecklistPlanResponse | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setIsLoading(true);
    setError(null);

    void loadChecklistPlan(sessionId)
      .then((response) => {
        if (!cancelled) {
          setPlan(response);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setError("Unable to load checklist plan.");
          setPlan(null);
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
    <section className="panel workstation-checklist" aria-label="Checklist Plan">
      <div className="workstation-panel-head">
        <p className="workstation-panel-eyebrow">Planning</p>
        <h2>Checklist Plan</h2>
      </div>

      {isLoading ? <p className="muted-text">Loading checklist plan...</p> : null}
      {error ? <p className="banner banner-error">{error}</p> : null}

      {!isLoading && !error && plan ? (
        <ul className="checklist-plan-list">
          {plan.domains.map((domain) => (
            <li key={domain.id}>
              <p className="checklist-domain-id">{domain.id}</p>
              <p className="muted-text">{domain.rationale}</p>
            </li>
          ))}
        </ul>
      ) : null}
    </section>
  );
}

export default ChecklistPanel;
