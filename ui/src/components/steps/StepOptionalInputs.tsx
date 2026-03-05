import { Eye, EyeOff, FileText, KeyRound } from "lucide-react";

import { LLM_DEGRADE } from "../../data/mockData";

type StepOptionalInputsProps = {
  specFileName: string;
  previousAuditFileName: string;
  customInvariantsFileName: string;
  llmApiKey: string;
  showApiKey: boolean;
  onSpecPick: () => void;
  onPreviousAuditPick: () => void;
  onCustomInvariantsPick: () => void;
  onLlmApiKeyChange: (value: string) => void;
  onToggleApiKeyVisibility: () => void;
};

function StepOptionalInputs({
  specFileName,
  previousAuditFileName,
  customInvariantsFileName,
  llmApiKey,
  showApiKey,
  onSpecPick,
  onPreviousAuditPick,
  onCustomInvariantsPick,
  onLlmApiKeyChange,
  onToggleApiKeyVisibility,
}: StepOptionalInputsProps): JSX.Element {
  return (
    <section className="step-card">
      <h2>Optional Inputs</h2>
      <p className="step-subtitle">All fields are optional. The audit runs without them.</p>
      <div className="optional-grid">
        <article className="panel">
          <h3>
            <FileText size={16} aria-hidden="true" />
            Specification (PDF / Markdown)
          </h3>
          <button type="button" className="dropzone" onClick={onSpecPick}>
            {specFileName ? `Selected: ${specFileName}` : "Drop file or browse"}
          </button>
        </article>

        <article className="panel">
          <h3>
            <FileText size={16} aria-hidden="true" />
            Previous audit report
          </h3>
          <button type="button" className="dropzone" onClick={onPreviousAuditPick}>
            {previousAuditFileName ? `Selected: ${previousAuditFileName}` : "Drop file or browse"}
          </button>
        </article>

        <article className="panel">
          <h3>
            <FileText size={16} aria-hidden="true" />
            Custom invariants.yaml
          </h3>
          <button type="button" className="dropzone" onClick={onCustomInvariantsPick}>
            {customInvariantsFileName ? `Selected: ${customInvariantsFileName}` : "Drop file or browse"}
          </button>
        </article>

        <article className="panel">
          <h3>
            <KeyRound size={16} aria-hidden="true" />
            LLM API Key (optional)
          </h3>
          <label>
            API key
            <div className="inline-input-row">
              <input
                type={showApiKey ? "text" : "password"}
                value={llmApiKey}
                onChange={(event) => onLlmApiKeyChange(event.target.value)}
                placeholder="sk-..."
              />
              <button
                type="button"
                className="inline-action"
                onClick={onToggleApiKeyVisibility}
                aria-label={showApiKey ? "Hide key" : "Show key"}
              >
                {showApiKey ? <EyeOff size={14} aria-hidden="true" /> : <Eye size={14} aria-hidden="true" />}
                {showApiKey ? "Hide key" : "Show key"}
              </button>
            </div>
          </label>

          {llmApiKey.trim().length === 0 ? (
            <div className="banner banner-info">
              <span>Without a key these features are degraded:</span>
              <ul>
                {LLM_DEGRADE.map((item) => (
                  <li key={item}>{item}</li>
                ))}
              </ul>
            </div>
          ) : (
            <div className="banner banner-success">LLM-enhanced analysis features are enabled.</div>
          )}
        </article>
      </div>
    </section>
  );
}

export default StepOptionalInputs;
