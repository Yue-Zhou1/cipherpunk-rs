import { useEffect, useState } from "react";

import AppHeader from "../../components/layout/AppHeader";
import FooterNav from "../../components/layout/FooterNav";
import StepConfiguration from "../../components/steps/StepConfiguration";
import StepExecution from "../../components/steps/StepExecution";
import StepOptionalInputs from "../../components/steps/StepOptionalInputs";
import StepResults from "../../components/steps/StepResults";
import StepSourceSelection from "../../components/steps/StepSourceSelection";
import StepWorkspaceConfirmation from "../../components/steps/StepWorkspaceConfirmation";
import { FINDINGS, STEPS, WORKSPACE_CRATES } from "../../data/mockData";
import {
  chooseSavePath,
  detectWorkspace,
  downloadOutput,
  exportAuditYaml,
  resolveSource,
  type BuildVariantSummary,
  type DetectWorkspaceResponse,
} from "../../ipc/commands";
import type { SourceInputIpc } from "../../ipc/commands";
import type { CrateRecord, OutputType, ResolvedCrateStatus, SourceMode, StepId } from "../../types";
import { createSessionFromConfirmation } from "./sessionFlow";

type SourceFormState = {
  gitUrl: string;
  gitRef: string;
  localPath: string;
  localCommit: string;
  archiveFileName: string;
};

type ConfigMode = "manual" | "upload";

type ConfigFormState = {
  mode: ConfigMode;
  uploadFileName: string;
  targetCrates: string;
  excludeCrates: string;
  kaniTimeoutSecs: string;
  z3TimeoutSecs: string;
  cryptoZkEnabled: boolean;
  distributedEnabled: boolean;
};

type OptionalInputsState = {
  specFileName: string;
  previousAuditFileName: string;
  customInvariantsFileName: string;
  llmApiKey: string;
  showApiKey: boolean;
};

const INITIAL_SOURCE_FORM: SourceFormState = {
  gitUrl: "https://github.com/org/repo",
  gitRef: "a1b2c3d4ef5678",
  localPath: "/workspace/circomlib",
  localCommit: "c0ffeecafef00d",
  archiveFileName: "",
};

const INITIAL_CONFIG_FORM: ConfigFormState = {
  mode: "manual",
  uploadFileName: "",
  targetCrates: "crate-a, crate-b",
  excludeCrates: "test-utils",
  kaniTimeoutSecs: "300",
  z3TimeoutSecs: "120",
  cryptoZkEnabled: true,
  distributedEnabled: false,
};

const INITIAL_OPTIONAL_INPUTS: OptionalInputsState = {
  specFileName: "",
  previousAuditFileName: "",
  customInvariantsFileName: "",
  llmApiKey: "",
  showApiKey: false,
};

type WizardShellProps = {
  onSessionCreated: (sessionId: string) => void;
};

function WizardShell({ onSessionCreated }: WizardShellProps): JSX.Element {
  const [currentStep, setCurrentStep] = useState<StepId>(1);
  const [sourceMode, setSourceMode] = useState<SourceMode>("git");
  const [sourceForm, setSourceForm] = useState<SourceFormState>(INITIAL_SOURCE_FORM);
  const [configForm, setConfigForm] = useState<ConfigFormState>(INITIAL_CONFIG_FORM);
  const [optionalInputs, setOptionalInputs] = useState<OptionalInputsState>(INITIAL_OPTIONAL_INPUTS);
  const [crateDecisions, setCrateDecisions] = useState<
    Partial<Record<string, ResolvedCrateStatus>>
  >({});
  const [selectedFindingId, setSelectedFindingId] = useState<string>(FINDINGS[0].id);
  const [auditId, setAuditId] = useState("audit-20260305-a1b2c3d4");
  const [branchResolutionBanner, setBranchResolutionBanner] = useState<string | null>(null);
  const [workspaceCrates, setWorkspaceCrates] = useState<CrateRecord[]>(WORKSPACE_CRATES);
  const [workspaceFrameworks, setWorkspaceFrameworks] = useState<string[]>([]);
  const [workspaceWarnings, setWorkspaceWarnings] = useState<string[]>([]);
  const [workspaceBuildMatrix, setWorkspaceBuildMatrix] = useState<BuildVariantSummary[]>([]);
  const [isWorkspaceLoading, setIsWorkspaceLoading] = useState(false);
  const [workspaceError, setWorkspaceError] = useState<string | null>(null);

  const [isStartingAudit, setIsStartingAudit] = useState(false);
  const [startError, setStartError] = useState<string | null>(null);
  const [isExportingAuditYaml, setIsExportingAuditYaml] = useState(false);
  const [exportError, setExportError] = useState<string | null>(null);
  const [exportMessage, setExportMessage] = useState<string | null>(null);
  const [generatedYamlMessage, setGeneratedYamlMessage] = useState<string | null>(null);

  const showFooter = currentStep <= 4;
  const canBack = currentStep > 1;
  const canNext =
    currentStep < 4 && isStepValid(currentStep, sourceMode, sourceForm, configForm);

  useEffect(() => {
    if (currentStep !== 4) {
      return;
    }

    let cancelled = false;
    setWorkspaceError(null);
    setIsWorkspaceLoading(true);

    void loadWorkspacePreview(
      sourceMode,
      sourceForm,
      (value) => {
        if (!cancelled) {
          setBranchResolutionBanner(value);
        }
      },
      (value) => {
        if (!cancelled) {
          setWorkspaceCrates(value);
        }
      },
      (value) => {
        if (!cancelled) {
          setWorkspaceFrameworks(value);
        }
      },
      (value) => {
        if (!cancelled) {
          setWorkspaceWarnings(value);
        }
      },
      (value) => {
        if (!cancelled) {
          setWorkspaceBuildMatrix(value);
        }
      }
    )
      .catch((error) => {
        if (!cancelled) {
          setWorkspaceError(errorMessage(error, "Unable to load workspace summary."));
        }
      })
      .finally(() => {
        if (!cancelled) {
          setIsWorkspaceLoading(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [
    currentStep,
    sourceMode,
    sourceForm.archiveFileName,
    sourceForm.gitRef,
    sourceForm.gitUrl,
    sourceForm.localCommit,
    sourceForm.localPath,
  ]);

  return (
    <div className="desktop-app-shell">
      <AppHeader steps={STEPS} currentStep={currentStep} onStepSelect={setCurrentStep} />

      <main className="content-frame">
        {renderStep({
          currentStep,
          sourceMode,
          onSourceModeChange: setSourceMode,
          sourceForm,
          onSourceFormChange: (patch) =>
            setSourceForm((previous) => ({ ...previous, ...patch })),
          configForm,
          onConfigFormChange: (patch) =>
            setConfigForm((previous) => ({ ...previous, ...patch })),
          optionalInputs,
          onOptionalInputsChange: (patch) =>
            setOptionalInputs((previous) => ({ ...previous, ...patch })),
          crateDecisions,
          onCrateDecision: (crateName, status) =>
            setCrateDecisions((previous) => ({
              ...previous,
              [crateName]: status,
            })),
          onStartAudit: async () => {
            setStartError(null);
            setIsStartingAudit(true);

            try {
              await loadWorkspacePreview(
                sourceMode,
                sourceForm,
                setBranchResolutionBanner,
                setWorkspaceCrates,
                setWorkspaceFrameworks,
                setWorkspaceWarnings,
                setWorkspaceBuildMatrix
              );

              const ambiguousCrates = Object.fromEntries(
                workspaceCrates.map((entry) => {
                  const status = crateDecisions[entry.name] ?? entry.status;
                  return [entry.name, status === "in_scope"];
                })
              );

              const response = await createSessionFromConfirmation({
                confirmed: true,
                ambiguousCrates,
              });
              setAuditId(response.auditId);
              onSessionCreated(response.sessionId);
            } catch (error) {
              setStartError(errorMessage(error, "Unable to start audit. Please retry."));
            } finally {
              setIsStartingAudit(false);
            }
          },
          isStartingAudit,
          startError,
          onExportAuditYaml: async () => {
            setExportError(null);
            setExportMessage(null);
            setIsExportingAuditYaml(true);

            try {
              const destination = (await chooseSavePath("audit.yaml")) ?? "audit.yaml";
              await exportAuditYaml(destination);
              setExportMessage("audit.yaml exported");
            } catch {
              setExportError("Unable to export audit.yaml. Please retry.");
            } finally {
              setIsExportingAuditYaml(false);
            }
          },
          isExportingAuditYaml,
          exportError,
          exportMessage,
          generatedYamlMessage,
          onDownloadGeneratedYaml: async () => {
            setGeneratedYamlMessage(null);
            try {
              const destination =
                (await chooseSavePath("generated-audit.yaml")) ?? "generated-audit.yaml";
              await exportAuditYaml(destination);
              setGeneratedYamlMessage("generated audit.yaml downloaded");
            } catch {
              setGeneratedYamlMessage("failed to generate audit.yaml");
            }
          },
          selectedFindingId,
          onSelectFinding: setSelectedFindingId,
          onDownloadOutput: async (outputType) => {
            const defaultDestination = destinationPathFor(outputType);
            const destination = (await chooseSavePath(defaultDestination)) ?? defaultDestination;
            await downloadOutput(auditId, outputType, destination);
          },
          branchResolutionBanner,
          workspaceCrates,
          workspaceFrameworks,
          workspaceWarnings,
          workspaceBuildMatrix,
          isWorkspaceLoading,
          workspaceError,
          auditId,
        })}
      </main>

      {showFooter ? (
        <FooterNav
          onBack={() =>
            setCurrentStep((previous) =>
              previous > 1 ? ((previous - 1) as StepId) : previous
            )
          }
          onNext={() =>
            setCurrentStep((previous) =>
              previous < 4 &&
              isStepValid(previous, sourceMode, sourceForm, configForm)
                ? ((previous + 1) as StepId)
                : previous
            )
          }
          canBack={canBack}
          canNext={canNext}
        />
      ) : null}
    </div>
  );
}

type RenderStepOptions = {
  currentStep: StepId;
  sourceMode: SourceMode;
  onSourceModeChange: (mode: SourceMode) => void;
  sourceForm: SourceFormState;
  onSourceFormChange: (patch: Partial<SourceFormState>) => void;
  configForm: ConfigFormState;
  onConfigFormChange: (patch: Partial<ConfigFormState>) => void;
  optionalInputs: OptionalInputsState;
  onOptionalInputsChange: (patch: Partial<OptionalInputsState>) => void;
  crateDecisions: Partial<Record<string, ResolvedCrateStatus>>;
  onCrateDecision: (crateName: string, status: ResolvedCrateStatus) => void;
  onStartAudit: () => void;
  isStartingAudit: boolean;
  startError: string | null;
  onExportAuditYaml: () => void;
  isExportingAuditYaml: boolean;
  exportError: string | null;
  exportMessage: string | null;
  generatedYamlMessage: string | null;
  onDownloadGeneratedYaml: () => Promise<void>;
  selectedFindingId: string;
  onSelectFinding: (findingId: string) => void;
  onDownloadOutput: (outputType: OutputType) => Promise<void>;
  branchResolutionBanner: string | null;
  workspaceCrates: CrateRecord[];
  workspaceFrameworks: string[];
  workspaceWarnings: string[];
  workspaceBuildMatrix: BuildVariantSummary[];
  isWorkspaceLoading: boolean;
  workspaceError: string | null;
  auditId: string;
};

function renderStep(options: RenderStepOptions): JSX.Element {
  switch (options.currentStep) {
    case 1:
      return (
        <StepSourceSelection
          mode={options.sourceMode}
          onModeChange={options.onSourceModeChange}
          gitUrl={options.sourceForm.gitUrl}
          gitRef={options.sourceForm.gitRef}
          localPath={options.sourceForm.localPath}
          localCommit={options.sourceForm.localCommit}
          archiveFileName={options.sourceForm.archiveFileName}
          branchResolutionBanner={options.branchResolutionBanner}
          onGitUrlChange={(value) => options.onSourceFormChange({ gitUrl: value })}
          onGitRefChange={(value) => options.onSourceFormChange({ gitRef: value })}
          onLocalPathChange={(value) => options.onSourceFormChange({ localPath: value })}
          onArchiveSelect={() =>
            options.onSourceFormChange({ archiveFileName: "source-bundle.tar.gz" })
          }
        />
      );
    case 2:
      return (
        <StepConfiguration
          mode={options.configForm.mode}
          uploadFileName={options.configForm.uploadFileName}
          targetCrates={options.configForm.targetCrates}
          excludeCrates={options.configForm.excludeCrates}
          kaniTimeoutSecs={options.configForm.kaniTimeoutSecs}
          z3TimeoutSecs={options.configForm.z3TimeoutSecs}
          cryptoZkEnabled={options.configForm.cryptoZkEnabled}
          distributedEnabled={options.configForm.distributedEnabled}
          generatedYamlMessage={options.generatedYamlMessage}
          onModeChange={(mode) => options.onConfigFormChange({ mode })}
          onUploadSelect={() => options.onConfigFormChange({ uploadFileName: "audit.yaml" })}
          onTargetCratesChange={(value) =>
            options.onConfigFormChange({ targetCrates: value })
          }
          onExcludeCratesChange={(value) =>
            options.onConfigFormChange({ excludeCrates: value })
          }
          onKaniTimeoutChange={(value) =>
            options.onConfigFormChange({ kaniTimeoutSecs: value })
          }
          onZ3TimeoutChange={(value) =>
            options.onConfigFormChange({ z3TimeoutSecs: value })
          }
          onCryptoZkToggle={(value) =>
            options.onConfigFormChange({ cryptoZkEnabled: value })
          }
          onDistributedToggle={(value) =>
            options.onConfigFormChange({ distributedEnabled: value })
          }
          onDownloadGeneratedYaml={options.onDownloadGeneratedYaml}
        />
      );
    case 3:
      return (
        <StepOptionalInputs
          specFileName={options.optionalInputs.specFileName}
          previousAuditFileName={options.optionalInputs.previousAuditFileName}
          customInvariantsFileName={options.optionalInputs.customInvariantsFileName}
          llmApiKey={options.optionalInputs.llmApiKey}
          showApiKey={options.optionalInputs.showApiKey}
          onSpecPick={() => options.onOptionalInputsChange({ specFileName: "specification.md" })}
          onPreviousAuditPick={() =>
            options.onOptionalInputsChange({ previousAuditFileName: "prior-audit.pdf" })
          }
          onCustomInvariantsPick={() =>
            options.onOptionalInputsChange({ customInvariantsFileName: "invariants.yaml" })
          }
          onLlmApiKeyChange={(value) => options.onOptionalInputsChange({ llmApiKey: value })}
          onToggleApiKeyVisibility={() =>
            options.onOptionalInputsChange({
              showApiKey: !options.optionalInputs.showApiKey,
            })
          }
        />
      );
    case 4:
      return (
        <StepWorkspaceConfirmation
          crates={options.workspaceCrates}
          frameworks={options.workspaceFrameworks}
          warnings={options.workspaceWarnings}
          buildMatrix={options.workspaceBuildMatrix}
          isWorkspaceLoading={options.isWorkspaceLoading}
          workspaceError={options.workspaceError}
          decisions={options.crateDecisions}
          onDecision={options.onCrateDecision}
          onStartAudit={options.onStartAudit}
          isStartingAudit={options.isStartingAudit}
          startError={options.startError}
          onExportAuditYaml={options.onExportAuditYaml}
          isExportingAuditYaml={options.isExportingAuditYaml}
          exportError={options.exportError}
          exportMessage={options.exportMessage}
        />
      );
    case 5:
      return <StepExecution auditId={options.auditId} />;
    case 6:
      return (
        <StepResults
          findings={FINDINGS}
          selectedFindingId={options.selectedFindingId}
          onSelectFinding={options.onSelectFinding}
          onDownloadOutput={options.onDownloadOutput}
        />
      );
    default:
      return <StepExecution auditId={options.auditId} />;
  }
}

function isStepValid(
  step: StepId,
  sourceMode: SourceMode,
  sourceForm: SourceFormState,
  configForm: ConfigFormState
): boolean {
  if (step === 1) {
    if (sourceMode === "git") {
      return (
        sourceForm.gitUrl.trim().length > 0 && sourceForm.gitRef.trim().length > 0
      );
    }

    if (sourceMode === "local") {
      return sourceForm.localPath.trim().length > 0;
    }

    return sourceForm.archiveFileName.trim().length > 0;
  }

  if (step === 2) {
    if (configForm.mode === "upload") {
      return configForm.uploadFileName.trim().length > 0;
    }

    return (
      configForm.targetCrates.trim().length > 0 &&
      isPositiveNumber(configForm.kaniTimeoutSecs) &&
      isPositiveNumber(configForm.z3TimeoutSecs)
    );
  }

  return true;
}

function isPositiveNumber(value: string): boolean {
  const parsed = Number(value);
  return Number.isFinite(parsed) && parsed > 0;
}

function errorMessage(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message.trim().length > 0) {
    return error.message;
  }
  return fallback;
}

function destinationPathFor(outputType: OutputType): string {
  const suffixByType: Record<OutputType, string> = {
    executive_pdf: "report-executive.pdf",
    technical_pdf: "report-technical.pdf",
    evidence_pack_zip: "evidence-pack.zip",
    findings_sarif: "findings.sarif",
    findings_json: "findings.json",
    regression_tests_zip: "regression-tests.zip",
  };

  return suffixByType[outputType];
}

function sourceInputFor(
  sourceMode: SourceMode,
  sourceForm: SourceFormState
): SourceInputIpc {
  if (sourceMode === "git") {
    return {
      kind: "git",
      value: sourceForm.gitUrl,
      commitOrRef: sourceForm.gitRef,
    };
  }

  if (sourceMode === "local") {
    return {
      kind: "local",
      value: sourceForm.localPath,
      commitOrRef: sourceForm.localCommit,
    };
  }

  return {
    kind: "archive",
    value: sourceForm.archiveFileName,
  };
}

async function loadWorkspacePreview(
  sourceMode: SourceMode,
  sourceForm: SourceFormState,
  onBanner: (value: string | null) => void,
  onCrates: (value: CrateRecord[]) => void,
  onFrameworks: (value: string[]) => void,
  onWarnings: (value: string[]) => void,
  onBuildMatrix: (value: BuildVariantSummary[]) => void
): Promise<void> {
  const sourceResponse = await resolveSource(sourceInputFor(sourceMode, sourceForm));
  onBanner(sourceResponse.branchResolutionBanner ?? null);

  const workspace = await detectWorkspace();
  applyWorkspaceSummary(workspace, onCrates, onFrameworks, onWarnings, onBuildMatrix);
}

function applyWorkspaceSummary(
  workspace: DetectWorkspaceResponse,
  onCrates: (value: CrateRecord[]) => void,
  onFrameworks: (value: string[]) => void,
  onWarnings: (value: string[]) => void,
  onBuildMatrix: (value: BuildVariantSummary[]) => void
): void {
  onCrates(
    workspace.crates.map((crateRecord) => ({
      name: crateRecord.name,
      status: crateRecord.status,
      reason: crateRecord.reason,
    }))
  );
  onFrameworks(workspace.frameworks);
  onWarnings(workspace.warnings);
  onBuildMatrix(workspace.buildMatrix);
}

export default WizardShell;
