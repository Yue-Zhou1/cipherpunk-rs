import { Copy, Download } from "lucide-react";
import { useMemo, useState } from "react";

import { OUTPUT_BUTTONS } from "../../data/mockData";
import type { FindingCategory, FindingRecord, FindingSeverity, OutputType } from "../../types";

type StepResultsProps = {
  findings: FindingRecord[];
  selectedFindingId: string;
  onSelectFinding: (findingId: string) => void;
  onDownloadOutput: (outputType: OutputType) => Promise<void>;
};

function StepResults({
  findings,
  selectedFindingId,
  onSelectFinding,
  onDownloadOutput,
}: StepResultsProps): JSX.Element {
  const [severityFilter, setSeverityFilter] = useState<"All" | FindingSeverity>("All");
  const [frameworkFilter, setFrameworkFilter] = useState<string>("All");
  const [categoryFilter, setCategoryFilter] = useState<"All" | FindingCategory>("All");
  const [selectedFileName, setSelectedFileName] = useState<string | null>(null);
  const [copyMessage, setCopyMessage] = useState<string | null>(null);
  const [downloadMessage, setDownloadMessage] = useState<string | null>(null);

  const filteredFindings = useMemo(
    () =>
      findings.filter((finding) => {
        const severityOk = severityFilter === "All" || finding.severity === severityFilter;
        const frameworkOk = frameworkFilter === "All" || finding.framework === frameworkFilter;
        const categoryOk = categoryFilter === "All" || finding.category === categoryFilter;
        return severityOk && frameworkOk && categoryOk;
      }),
    [categoryFilter, findings, frameworkFilter, severityFilter]
  );

  const selectedFinding =
    filteredFindings.find((finding) => finding.id === selectedFindingId) ?? filteredFindings[0] ?? findings[0];

  const frameworkOptions = useMemo(
    () => ["All", ...Array.from(new Set(findings.map((finding) => finding.framework)))],
    [findings]
  );

  const selectedFile =
    selectedFinding.evidenceFiles.find((file) => file.name === selectedFileName) ??
    selectedFinding.evidenceFiles[0] ??
    null;

  async function handleCopyScript(): Promise<void> {
    try {
      await navigator.clipboard.writeText(selectedFinding.reproduceScript);
      setCopyMessage("Copied");
      setTimeout(() => setCopyMessage(null), 1500);
    } catch {
      setCopyMessage("Copy failed");
      setTimeout(() => setCopyMessage(null), 1500);
    }
  }

  return (
    <section className="step-card results-view">
      <h2>Audit Results - Score 65/100</h2>
      <div className="results-grid">
        <aside className="panel finding-list">
          <h3>Findings</h3>
          <div className="filter-grid">
            <label>
              Severity filter
              <select
                aria-label="Severity filter"
                value={severityFilter}
                onChange={(event) => setSeverityFilter(event.target.value as typeof severityFilter)}
              >
                <option value="All">All</option>
                <option value="Critical">Critical</option>
                <option value="High">High</option>
                <option value="Medium">Medium</option>
                <option value="Low">Low</option>
                <option value="Observation">Observation</option>
              </select>
            </label>
            <label>
              Framework filter
              <select
                aria-label="Framework filter"
                value={frameworkFilter}
                onChange={(event) => setFrameworkFilter(event.target.value)}
              >
                {frameworkOptions.map((framework) => (
                  <option key={framework} value={framework}>
                    {framework}
                  </option>
                ))}
              </select>
            </label>
            <label>
              Category filter
              <select
                aria-label="Category filter"
                value={categoryFilter}
                onChange={(event) => setCategoryFilter(event.target.value as typeof categoryFilter)}
              >
                <option value="All">All</option>
                <option value="Crypto Misuse">Crypto Misuse</option>
                <option value="Distributed">Distributed</option>
                <option value="Economic">Economic</option>
              </select>
            </label>
          </div>

          <p className="muted-text">Showing {filteredFindings.length} finding{filteredFindings.length === 1 ? "" : "s"}</p>

          {filteredFindings.map((finding) => (
            <button
              key={finding.id}
              type="button"
              className={`finding-item ${selectedFinding.id === finding.id ? "active" : ""}`}
              onClick={() => {
                onSelectFinding(finding.id);
                setSelectedFileName(null);
              }}
            >
              <div className="finding-item-head">
                <span className={`severity-chip severity-${finding.severity.toLowerCase()}`}>{finding.severity}</span>
                {finding.llmGenerated ? <span className="llm-badge">LLM Generated</span> : null}
              </div>
              <strong>{finding.id}</strong> - {finding.title}
            </button>
          ))}
        </aside>

        <article className="panel">
          <h3>Detail</h3>
          <p>Severity: {selectedFinding.severity}</p>
          <p>Framework: {selectedFinding.framework}</p>
          <p>Status: {selectedFinding.verificationStatus}</p>
          <p>Rule: {selectedFinding.ruleId}</p>
          <p>Affected: {selectedFinding.affected}</p>
          <p>{selectedFinding.description}</p>
          <p><strong>Recommendation:</strong> {selectedFinding.recommendation}</p>
          <pre>
            <code>{selectedFinding.codeSnippet}</code>
          </pre>

          {selectedFinding.cdg ? (
            <section className="embedded-view">
              <h4>CDG View</h4>
              <div className="mini-graph">
                {selectedFinding.cdg.nodes.map((node) => (
                  <span
                    key={node.id}
                    className={`graph-node ${node.risk ? "risk" : ""}`}
                  >
                    {node.id}
                  </span>
                ))}
              </div>
            </section>
          ) : null}

          {selectedFinding.trace ? (
            <section className="embedded-view">
              <h4>Trace Viewer</h4>
              <p>
                Seed: {selectedFinding.trace.seed} | Duration: {selectedFinding.trace.durationTicks} ticks
              </p>
              <div className="trace-table">
                {selectedFinding.trace.events.map((event) => (
                  <p key={`${event.tick}-${event.node}-${event.event}`} className={event.violation ? "trace-violation" : ""}>
                    {event.tick} {event.node} {event.event}
                  </p>
                ))}
              </div>
              <p>{selectedFinding.trace.violationSummary}</p>
            </section>
          ) : null}
        </article>

        <aside className="panel">
          <h3>Evidence</h3>
          <div className="code-toolbar">
            <span>reproduce.sh</span>
            <button type="button" className="inline-action" onClick={handleCopyScript}>
              <Copy size={14} aria-hidden="true" />
              Copy
            </button>
          </div>
          <pre>
            <code>{selectedFinding.reproduceScript}</code>
          </pre>
          {copyMessage ? <p className="muted-text">{copyMessage}</p> : null}

          <div className="files-section">
            <p>Files</p>
            <div className="file-list">
              {selectedFinding.evidenceFiles.map((file) => (
                <button
                  key={file.name}
                  type="button"
                  className={`file-item ${selectedFile?.name === file.name ? "active" : ""}`}
                  onClick={() => setSelectedFileName(file.name)}
                >
                  {file.name}
                </button>
              ))}
            </div>
            {selectedFile ? (
              <pre>
                <code>{selectedFile.content}</code>
              </pre>
            ) : null}
          </div>

          <div className="export-stack">
            {OUTPUT_BUTTONS.map((output) => (
              <button
                key={output.type}
                type="button"
                className="export-button"
                onClick={async () => {
                  await onDownloadOutput(output.type);
                  setDownloadMessage(`${output.label} ready`);
                }}
              >
                <Download size={14} aria-hidden="true" />
                {output.label}
              </button>
            ))}
          </div>
          {downloadMessage ? <p className="muted-text">{downloadMessage}</p> : null}
        </aside>
      </div>
    </section>
  );
}

export default StepResults;
