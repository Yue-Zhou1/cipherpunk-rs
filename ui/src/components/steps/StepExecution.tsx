import { ChevronDown, ChevronUp } from "lucide-react";
import { useEffect, useMemo, useState } from "react";

import {
  subscribeExecutionUpdates,
  type ExecutionCounts,
  type ExecutionLogEntry,
  type ExecutionNode,
  type ExecutionUpdateEvent,
} from "../../ipc/commands";

const INITIAL_NODES: ExecutionNode[] = [
  { name: "Intake", channel: "intake", status: "done" },
  { name: "Rule Eval", channel: "rules", status: "running" },
  { name: "Z3 Check", channel: "z3", status: "waiting" },
  { name: "Report", channel: "report", status: "waiting" },
];

const INITIAL_COUNTS: ExecutionCounts = {
  critical: 0,
  high: 1,
  medium: 2,
  low: 0,
  observation: 1,
};

const INITIAL_LOGS: ExecutionLogEntry[] = [
  { timestamp: "14:23:01", channel: "intake", message: "Cloning repo" },
  { timestamp: "14:23:05", channel: "intake", message: "12 crates detected" },
  { timestamp: "14:23:08", channel: "rules", message: "Evaluating crypto misuse rules" },
];

type StepExecutionProps = {
  auditId: string;
};

function StepExecution({ auditId }: StepExecutionProps): JSX.Element {
  const [nodes, setNodes] = useState<ExecutionNode[]>(INITIAL_NODES);
  const [counts, setCounts] = useState<ExecutionCounts>(INITIAL_COUNTS);
  const [latestFinding, setLatestFinding] = useState(
    "F-ZK-0042 High - canonicality check missing"
  );
  const [logs, setLogs] = useState<ExecutionLogEntry[]>(INITIAL_LOGS);
  const [showLogs, setShowLogs] = useState(true);
  const [logFilter, setLogFilter] = useState<"all" | ExecutionNode["channel"]>("all");

  const visibleLogs = useMemo(() => {
    if (logFilter === "all") {
      return logs;
    }

    return logs.filter((entry) => entry.channel === logFilter);
  }, [logFilter, logs]);

  useEffect(() => {
    const unsubscribe = subscribeExecutionUpdates(
      auditId,
      (update: ExecutionUpdateEvent) => {
        setNodes(update.nodes);
        setCounts(update.counts);
        setLogs(update.logs);
        setLatestFinding(update.latestFinding);
      }
    );

    return () => unsubscribe();
  }, [auditId]);

  return (
    <section className="step-card execution-view">
      <h2>Audit Running - circomlib @ a1b2c3</h2>
      <div className="execution-grid">
        <div className="panel">
          <h3>Pipeline DAG</h3>
          <div className="dag-grid">
            {nodes.map((node) => (
              <article className={`dag-node ${node.status}`} key={node.name}>
                <p>{node.name}</p>
                <small>{node.status}</small>
              </article>
            ))}
          </div>

          <div className="log-toolbar">
            <label>
              Log filter
              <select value={logFilter} onChange={(event) => setLogFilter(event.target.value as typeof logFilter)}>
                <option value="all">All channels</option>
                <option value="intake">Intake</option>
                <option value="rules">Rules</option>
                <option value="z3">Z3</option>
                <option value="report">Report</option>
              </select>
            </label>
            <button
              type="button"
              className="inline-action"
              onClick={() => setShowLogs((value) => !value)}
            >
              {showLogs ? <ChevronUp size={14} aria-hidden="true" /> : <ChevronDown size={14} aria-hidden="true" />}
              {showLogs ? "Hide logs" : "Show logs"}
            </button>
          </div>

          {showLogs ? (
            <div className="log-stream" role="log" aria-label="Live logs">
              {visibleLogs.map((entry) => (
                <code key={`${entry.timestamp}-${entry.channel}-${entry.message}`}>
                  {entry.timestamp} [{entry.channel}] {entry.message}
                </code>
              ))}
            </div>
          ) : null}
        </div>

        <div className="panel">
          <h3>Findings (live)</h3>
          <ul className="finding-feed">
            <li><strong>Critical:</strong> {counts.critical}</li>
            <li><strong>High:</strong> {counts.high}</li>
            <li><strong>Medium:</strong> {counts.medium}</li>
            <li><strong>Low:</strong> {counts.low}</li>
            <li><strong>Observation:</strong> {counts.observation}</li>
          </ul>
          <div className="latest-card">
            <span>{latestFinding}</span>
          </div>
        </div>
      </div>
    </section>
  );
}

export default StepExecution;
