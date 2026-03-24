import { useEffect, useMemo, useState } from "react";

import {
  loadActivitySummary,
  type ActivitySummary,
  type SessionConsoleEntry,
} from "../../ipc/commands";

type ActivityConsoleProps = {
  sessionId: string;
  entries: SessionConsoleEntry[];
};

type ActivityFilter = "all" | "llm" | "tools" | "reviews" | "engines";

const FILTERS: ActivityFilter[] = ["all", "llm", "tools", "reviews", "engines"];

function filterLabel(filter: ActivityFilter): string {
  if (filter === "all") {
    return "All";
  }
  if (filter === "llm") {
    return "LLM";
  }
  if (filter === "tools") {
    return "Tools";
  }
  if (filter === "reviews") {
    return "Reviews";
  }
  return "Engines";
}

function eventMatchesFilter(entry: SessionConsoleEntry, filter: ActivityFilter): boolean {
  if (filter === "all") {
    return true;
  }
  if (filter === "llm") {
    return entry.source.startsWith("llm.");
  }
  if (filter === "tools") {
    return entry.source.startsWith("tool.");
  }
  if (filter === "reviews") {
    return entry.source.startsWith("review.");
  }
  return entry.source.startsWith("engine.");
}

function getCountForFilter(summary: ActivitySummary, filter: ActivityFilter): number {
  const llmCalls = summary.llmCalls ?? [];
  const toolActions = summary.toolActions ?? [];
  const reviewDecisions = summary.reviewDecisions ?? [];
  const engineOutcomes = summary.engineOutcomes ?? [];
  const totalEvents =
    typeof summary.totalEvents === "number"
      ? summary.totalEvents
      : llmCalls.reduce((sum, call) => sum + call.count, 0) +
        toolActions.reduce((sum, action) => sum + action.count, 0) +
        reviewDecisions.reduce((sum, decision) => sum + decision.count, 0) +
        engineOutcomes.length;

  if (filter === "all") {
    return totalEvents;
  }
  if (filter === "llm") {
    return llmCalls.reduce((sum, call) => sum + call.count, 0);
  }
  if (filter === "tools") {
    return toolActions.reduce((sum, action) => sum + action.count, 0);
  }
  if (filter === "reviews") {
    return reviewDecisions.reduce((sum, item) => sum + item.count, 0);
  }
  return engineOutcomes.length;
}

function ActivityConsole({ sessionId, entries }: ActivityConsoleProps): JSX.Element {
  const [filter, setFilter] = useState<ActivityFilter>("all");
  const [summary, setSummary] = useState<ActivitySummary | null>(null);

  useEffect(() => {
    let cancelled = false;

    const refreshSummary = (): void => {
      void loadActivitySummary(sessionId)
        .then((response) => {
          if (!cancelled) {
            setSummary(response);
          }
        })
        .catch(() => {
          if (!cancelled) {
            setSummary(null);
          }
        });
    };

    refreshSummary();
    const timer = window.setInterval(refreshSummary, 3000);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [sessionId]);

  const filteredEntries = useMemo(
    () => entries.filter((entry) => eventMatchesFilter(entry, filter)),
    [entries, filter]
  );
  const llmCalls = summary?.llmCalls ?? [];
  const toolActions = summary?.toolActions ?? [];
  const reviewDecisions = summary?.reviewDecisions ?? [];
  const engineOutcomes = summary?.engineOutcomes ?? [];

  const llmProviderBadge = llmCalls
    .flatMap((item) => item.providersUsed)
    .filter((value, index, all) => all.indexOf(value) === index)
    .join(", ");

  return (
    <section className="panel workstation-console" aria-label="Activity Console">
      <div className="workstation-panel-head">
        <p className="workstation-panel-eyebrow">Panel</p>
      </div>
      <div className="code-toolbar">
        <h2>Activity Console</h2>
        <span className="muted-text">{entries.length} events</span>
      </div>

      <div className="flex gap-1 px-3 py-1 border-b border-gray-200 bg-gray-50">
        {FILTERS.map((candidate) => (
          <button
            key={candidate}
            type="button"
            onClick={() => setFilter(candidate)}
            className={`text-xs px-2 py-0.5 rounded ${
              filter === candidate ? "bg-blue-100 text-blue-700" : "text-gray-600"
            }`}
          >
            {filterLabel(candidate)}
            {summary && candidate !== "all" ? (
              <span className="ml-1 text-gray-400">
                ({getCountForFilter(summary, candidate)})
              </span>
            ) : null}
          </button>
        ))}
      </div>

      {summary ? (
        <div className="px-3 py-2 text-xs text-gray-500 border-b">
          {llmCalls.reduce((sum, call) => sum + call.count, 0)} LLM calls ·{" "}
          {toolActions.reduce((sum, action) => sum + action.count, 0)} tool actions ·{" "}
          {reviewDecisions.reduce((sum, decision) => sum + decision.count, 0)} reviews ·{" "}
          {engineOutcomes.length} engines
        </div>
      ) : null}

      <p className="muted-text">
        Tracks tool runs and review actions (`confirm`, `reject`, `suppress`, `annotate`).
      </p>

      {filteredEntries.length === 0 ? (
        <p className="muted-text">No activity yet.</p>
      ) : (
        <div className="workstation-console-stream" role="log" aria-label="Session activity logs">
          {filteredEntries.map((entry, index) => (
            <div key={`${entry.timestamp}-${entry.source}-${index}`} className="console-row">
              <span className={`console-level ${entry.level}`}>{entry.level.toUpperCase()}</span>
              <code>
                [{entry.timestamp}] {entry.source}: {entry.message}
              </code>
              {entry.source === "llm.interaction" && llmProviderBadge ? (
                <span className="text-xs bg-purple-100 text-purple-700 rounded px-1 ml-1">
                  {llmProviderBadge}
                </span>
              ) : null}
              {entry.source.startsWith("tool.action") ? (
                <span
                  className={`text-xs rounded px-1 ml-1 ${
                    entry.level === "error"
                      ? "bg-red-100 text-red-700"
                      : "bg-green-100 text-green-700"
                  }`}
                >
                  {entry.level === "error" ? "Failed" : "Completed"}
                </span>
              ) : null}
            </div>
          ))}
        </div>
      )}
    </section>
  );
}

export default ActivityConsole;
