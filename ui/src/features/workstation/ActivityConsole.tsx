import type { SessionConsoleEntry } from "../../ipc/commands";

type ActivityConsoleProps = {
  entries: SessionConsoleEntry[];
};

function ActivityConsole({ entries }: ActivityConsoleProps): JSX.Element {
  return (
    <section className="panel workstation-console" aria-label="Activity Console">
      <div className="workstation-panel-head">
        <p className="workstation-panel-eyebrow">Panel</p>
      </div>
      <div className="code-toolbar">
        <h2>Activity Console</h2>
        <span className="muted-text">{entries.length} events</span>
      </div>

      {entries.length === 0 ? (
        <p className="muted-text">No activity yet.</p>
      ) : (
        <div className="workstation-console-stream" role="log" aria-label="Session activity logs">
          {entries.map((entry, index) => (
            <div key={`${entry.timestamp}-${entry.source}-${index}`} className="console-row">
              <span className={`console-level ${entry.level}`}>{entry.level.toUpperCase()}</span>
              <code>
                [{entry.timestamp}] {entry.source}: {entry.message}
              </code>
            </div>
          ))}
        </div>
      )}
    </section>
  );
}

export default ActivityConsole;
