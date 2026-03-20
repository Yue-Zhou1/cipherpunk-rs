import { useState } from "react";
import { BrowserRouter, Navigate, Route, Routes, useNavigate, useParams } from "react-router-dom";

import WizardShell from "./features/wizard/WizardShell";
import WorkstationShell from "./features/workstation/WorkstationShell";
import { getTransport } from "./ipc/transport";
import type { AppMode } from "./types";

function WebWizardRoute(): JSX.Element {
  const navigate = useNavigate();

  return (
    <WizardShell
      onSessionCreated={(sessionId) => {
        navigate(`/sessions/${encodeURIComponent(sessionId)}`, { replace: true });
      }}
    />
  );
}

function WebWorkstationRoute(): JSX.Element {
  const params = useParams<{ sessionId: string }>();
  if (!params.sessionId) {
    return <Navigate to="/wizard" replace />;
  }

  return <WorkstationShell sessionId={decodeURIComponent(params.sessionId)} />;
}

function App(): JSX.Element {
  if (getTransport().kind === "http") {
    return (
      <BrowserRouter>
        <Routes>
          <Route path="/" element={<Navigate to="/wizard" replace />} />
          <Route path="/wizard" element={<WebWizardRoute />} />
          <Route path="/sessions/:sessionId" element={<WebWorkstationRoute />} />
          <Route path="*" element={<Navigate to="/wizard" replace />} />
        </Routes>
      </BrowserRouter>
    );
  }

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
