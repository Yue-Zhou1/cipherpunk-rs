import { useState } from "react";

import WizardShell from "./features/wizard/WizardShell";
import type { AppMode } from "./types";

function App(): JSX.Element {
  const [mode, setMode] = useState<AppMode>({ kind: "wizard" });

  if (mode.kind === "wizard") {
    return (
      <WizardShell
        onSessionCreated={(sessionId) => setMode({ kind: "workstation", sessionId })}
      />
    );
  }

  return (
    <div className="desktop-app-shell">
      <main className="content-frame">
        <section className="step-card">
          <h2>Workstation</h2>
          <p>Session ID: {mode.sessionId}</p>
        </section>
      </main>
    </div>
  );
}

export default App;
