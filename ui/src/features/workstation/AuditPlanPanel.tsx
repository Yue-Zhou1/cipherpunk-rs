import { useEffect, useState, type ReactNode } from "react";

import { loadAuditPlan, type AuditPlanResponse } from "../../ipc/commands";

type AuditPlanPanelProps = {
  sessionId: string;
};

function AuditPlanPanel({ sessionId }: AuditPlanPanelProps): JSX.Element {
  const [plan, setPlan] = useState<AuditPlanResponse | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setIsLoading(true);
    setError(null);

    void loadAuditPlan(sessionId)
      .then((response) => {
        if (!cancelled) {
          setPlan(response);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setError("No audit plan generated yet.");
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
    <section className="panel workstation-audit-plan" aria-label="Audit Plan">
      <div className="workstation-panel-head">
        <p className="workstation-panel-eyebrow">Planning</p>
        <h2>Audit Plan</h2>
      </div>

      {isLoading ? <p className="muted-text">Loading plan...</p> : null}
      {error ? <p className="muted-text">{error}</p> : null}

      {!isLoading && !error && plan ? (
        <div className="space-y-3">
          <p className="muted-text">{plan.rationale}</p>

          <Section title="Architecture Overview">
            <SubSection title="Assets" items={plan.overview.assets} />
            <SubSection title="Trust Boundaries" items={plan.overview.trustBoundaries} />
            <SubSection title="Hotspots" items={plan.overview.hotspots} />
          </Section>

          <Section title="Analysis Domains">
            {plan.domains.map((domain) => (
              <div key={domain.id} className="ml-2 mb-2">
                <span className="font-medium text-sm">{domain.id}</span>
                <p className="text-xs text-gray-500 ml-2">{domain.rationale}</p>
              </div>
            ))}
          </Section>

          <Section title="Recommended Tools">
            {plan.recommendedTools.map((tool) => (
              <div key={tool.tool} className="ml-2 mb-2">
                <span className="font-medium text-sm">{tool.tool}</span>
                <p className="text-xs text-gray-500 ml-2">{tool.rationale}</p>
              </div>
            ))}
          </Section>

          <Section title="Engines">
            <div className="ml-2 text-sm space-y-1">
              <div>
                Crypto/ZK: <Badge enabled={plan.engines.cryptoZk} />
              </div>
              <div>
                Distributed: <Badge enabled={plan.engines.distributed} />
              </div>
            </div>
          </Section>
        </div>
      ) : null}
    </section>
  );
}

function Section({ title, children }: { title: string; children: ReactNode }): JSX.Element {
  const [open, setOpen] = useState(true);
  return (
    <div className="border border-gray-200 rounded">
      <button
        type="button"
        onClick={() => setOpen((current) => !current)}
        className="w-full text-left px-3 py-2 text-sm font-medium bg-gray-50 hover:bg-gray-100"
      >
        {open ? "▾" : "▸"} {title}
      </button>
      {open ? <div className="px-3 py-2">{children}</div> : null}
    </div>
  );
}

function SubSection({ title, items }: { title: string; items: string[] }): JSX.Element | null {
  if (items.length === 0) {
    return null;
  }
  return (
    <div className="mb-2">
      <span className="text-xs font-medium text-gray-600">{title}</span>
      <ul className="ml-2">
        {items.map((item, index) => (
          <li key={`${title}-${index}`} className="text-xs text-gray-700">
            {item}
          </li>
        ))}
      </ul>
    </div>
  );
}

function Badge({ enabled }: { enabled: boolean }): JSX.Element {
  return (
    <span
      className={`text-xs px-1 rounded ${
        enabled ? "bg-green-100 text-green-700" : "bg-gray-100 text-gray-500"
      }`}
    >
      {enabled ? "enabled" : "disabled"}
    </span>
  );
}

export default AuditPlanPanel;
