import { useState } from "react";

import WizardShell from "./features/wizard/WizardShell";
import WorkstationShell from "./features/workstation/WorkstationShell";
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

  return <WorkstationShell sessionId={mode.sessionId} />;
}

export default App;
