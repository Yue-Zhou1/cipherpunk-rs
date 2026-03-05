import type { CrateRecord, ResolvedCrateStatus } from "../../types";

type BuildMatrixRow = {
  variant: string;
  features: string;
  estTime: string;
};

type StepWorkspaceConfirmationProps = {
  crates: CrateRecord[];
  frameworks: string[];
  warnings: string[];
  buildMatrix: BuildMatrixRow[];
  isWorkspaceLoading: boolean;
  workspaceError: string | null;
  decisions: Partial<Record<string, ResolvedCrateStatus>>;
  onDecision: (crateName: string, status: ResolvedCrateStatus) => void;
  onStartAudit: () => void;
  isStartingAudit: boolean;
  startError: string | null;
  onExportAuditYaml: () => void;
  isExportingAuditYaml: boolean;
  exportError: string | null;
  exportMessage: string | null;
};

function StepWorkspaceConfirmation({
  crates,
  frameworks,
  warnings,
  buildMatrix,
  isWorkspaceLoading,
  workspaceError,
  decisions,
  onDecision,
  onStartAudit,
  isStartingAudit,
  startError,
  onExportAuditYaml,
  isExportingAuditYaml,
  exportError,
  exportMessage,
}: StepWorkspaceConfirmationProps): JSX.Element {
  return (
    <section className="step-card">
      <h2>Workspace Confirmation</h2>
      {isWorkspaceLoading ? (
        <div className="banner banner-info">Resolving source and loading workspace summary...</div>
      ) : null}
      {workspaceError ? <div className="banner banner-error">{workspaceError}</div> : null}
      {warnings.length > 0 ? (
        <div className="banner-stack">
          {warnings.map((warning) => (
            <div key={warning} className="banner banner-warning">
              {warning}
            </div>
          ))}
        </div>
      ) : null}

      {frameworks.length > 0 ? (
        <div className="chip-row">
          {frameworks.map((framework) => (
            <span className="chip" key={framework}>
              {framework}
            </span>
          ))}
        </div>
      ) : null}

      <div className="table-card">
        <table>
          <thead>
            <tr>
              <th>Crate</th>
              <th>Status</th>
              <th>Action</th>
            </tr>
          </thead>
          <tbody>
            {crates.map((crate) => {
              const status =
                crate.status === "ambiguous" ? decisions[crate.name] ?? "ambiguous" : crate.status;

              return (
                <tr key={crate.name}>
                  <td>{crate.name}</td>
                  <td className={statusClass(status)}>{statusLabel(status)}</td>
                  <td>
                    {crate.status === "ambiguous" ? (
                      <>
                        <button
                          type="button"
                          className={`inline-action ${status === "in_scope" ? "active" : ""}`}
                          onClick={() => onDecision(crate.name, "in_scope")}
                        >
                          Include
                        </button>
                        <button
                          type="button"
                          className={`inline-action ${status === "excluded" ? "active" : ""}`}
                          onClick={() => onDecision(crate.name, "excluded")}
                        >
                          Exclude
                        </button>
                      </>
                    ) : (
                      crate.reason ?? "-"
                    )}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>

      <div className="table-card">
        <table>
          <thead>
            <tr>
              <th>Variant</th>
              <th>Features</th>
              <th>Est. Time</th>
            </tr>
          </thead>
          <tbody>
            {buildMatrix.length > 0 ? (
              buildMatrix.map((row) => (
                <tr key={`${row.variant}:${row.features}`}>
                  <td>{row.variant}</td>
                  <td>{row.features}</td>
                  <td>{row.estTime}</td>
                </tr>
              ))
            ) : (
              <tr>
                <td colSpan={3} className="muted">
                  No build variants detected yet.
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>

      <div className="action-row">
        <button
          type="button"
          className="nav-button nav-button-ghost"
          onClick={onExportAuditYaml}
          disabled={isExportingAuditYaml}
        >
          {isExportingAuditYaml ? "Exporting..." : "Export audit.yaml"}
        </button>
        <button
          type="button"
          className="nav-button nav-button-primary"
          onClick={onStartAudit}
          disabled={isStartingAudit}
        >
          {isStartingAudit ? "Starting..." : "Confirm and Start Audit"}
        </button>
      </div>
      {exportMessage ? <div className="banner banner-success">{exportMessage}</div> : null}
      {exportError ? <div className="banner banner-error">{exportError}</div> : null}
      {startError ? <div className="banner banner-error">{startError}</div> : null}
    </section>
  );
}

function statusLabel(status: CrateRecord["status"] | ResolvedCrateStatus): string {
  if (status === "in_scope") {
    return "In Scope";
  }

  if (status === "excluded") {
    return "Excluded";
  }

  return "Ambiguous";
}

function statusClass(status: CrateRecord["status"] | ResolvedCrateStatus): string {
  if (status === "in_scope") {
    return "ok";
  }

  if (status === "excluded") {
    return "muted";
  }

  return "warn";
}

export default StepWorkspaceConfirmation;
