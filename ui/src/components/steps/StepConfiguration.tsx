import { Upload } from "lucide-react";

type ConfigMode = "manual" | "upload";

type StepConfigurationProps = {
  mode: ConfigMode;
  uploadFileName: string;
  targetCrates: string;
  excludeCrates: string;
  kaniTimeoutSecs: string;
  z3TimeoutSecs: string;
  cryptoZkEnabled: boolean;
  distributedEnabled: boolean;
  generatedYamlMessage: string | null;
  onModeChange: (mode: ConfigMode) => void;
  onUploadSelect: () => void;
  onTargetCratesChange: (value: string) => void;
  onExcludeCratesChange: (value: string) => void;
  onKaniTimeoutChange: (value: string) => void;
  onZ3TimeoutChange: (value: string) => void;
  onCryptoZkToggle: (value: boolean) => void;
  onDistributedToggle: (value: boolean) => void;
  onDownloadGeneratedYaml: () => void;
};

function StepConfiguration({
  mode,
  uploadFileName,
  targetCrates,
  excludeCrates,
  kaniTimeoutSecs,
  z3TimeoutSecs,
  cryptoZkEnabled,
  distributedEnabled,
  generatedYamlMessage,
  onModeChange,
  onUploadSelect,
  onTargetCratesChange,
  onExcludeCratesChange,
  onKaniTimeoutChange,
  onZ3TimeoutChange,
  onCryptoZkToggle,
  onDistributedToggle,
  onDownloadGeneratedYaml,
}: StepConfigurationProps): JSX.Element {
  return (
    <section className="step-card">
      <h2>Audit Configuration</h2>

      <div className="pill-tabs" role="tablist" aria-label="Configuration mode">
        <button
          type="button"
          role="tab"
          aria-selected={mode === "upload"}
          className={`pill-tab ${mode === "upload" ? "active" : ""}`}
          onClick={() => onModeChange("upload")}
        >
          Upload audit.yaml
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={mode === "manual"}
          className={`pill-tab ${mode === "manual" ? "active" : ""}`}
          onClick={() => onModeChange("manual")}
        >
          Manual Form
        </button>
      </div>

      {mode === "upload" ? (
        <article className="panel">
          <h3>Upload audit.yaml</h3>
          <button type="button" className="dropzone" onClick={onUploadSelect}>
            <Upload size={18} aria-hidden="true" />
            <span>{uploadFileName ? `Selected: ${uploadFileName}` : "Drop file or click to browse"}</span>
          </button>
        </article>
      ) : (
        <article className="panel">
          <h3>Manual Form</h3>
          <div className="form-grid compact">
            <label>
              Target crates
              <input
                type="text"
                value={targetCrates}
                onChange={(event) => onTargetCratesChange(event.target.value)}
              />
            </label>
            <label>
              Exclude crates
              <input
                type="text"
                value={excludeCrates}
                onChange={(event) => onExcludeCratesChange(event.target.value)}
              />
            </label>
            <label>
              Kani timeout
              <input
                type="text"
                value={kaniTimeoutSecs}
                onChange={(event) => onKaniTimeoutChange(event.target.value)}
              />
            </label>
            <label>
              Z3 timeout
              <input
                type="text"
                value={z3TimeoutSecs}
                onChange={(event) => onZ3TimeoutChange(event.target.value)}
              />
            </label>
          </div>

          <div className="toggle-row">
            <label className="checkbox-line">
              <input
                type="checkbox"
                checked={cryptoZkEnabled}
                onChange={(event) => onCryptoZkToggle(event.target.checked)}
              />
              Crypto / ZK engine
            </label>
            <label className="checkbox-line">
              <input
                type="checkbox"
                checked={distributedEnabled}
                onChange={(event) => onDistributedToggle(event.target.checked)}
              />
              Distributed engine
            </label>
          </div>

          <button type="button" className="nav-button nav-button-ghost" onClick={onDownloadGeneratedYaml}>
            Download generated audit.yaml
          </button>
          {generatedYamlMessage ? <p className="muted-text">{generatedYamlMessage}</p> : null}
        </article>
      )}
    </section>
  );
}

export default StepConfiguration;
