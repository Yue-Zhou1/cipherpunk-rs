import { Check, HelpCircle, Shield } from "lucide-react";

import type { StepDefinition, StepId } from "../../types";

type AppHeaderProps = {
  steps: StepDefinition[];
  currentStep: StepId;
  onStepSelect: (step: StepId) => void;
};

function AppHeader({ steps, currentStep, onStepSelect }: AppHeaderProps): JSX.Element {
  const activeStep = steps.find((step) => step.id === currentStep) ?? steps[0];

  return (
    <header className="top-bar" role="banner">
      <div className="brand-mark">
        <Shield size={18} aria-hidden="true" />
        <div>
          <p className="brand-eyebrow">Audit Agent</p>
          <h1>{activeStep.title}</h1>
        </div>
      </div>

      <nav aria-label="Workflow steps" className="stepper-wrapper">
        <ol className="stepper-list">
          {steps.map((step, index) => {
            const status =
              step.id < currentStep ? "completed" : step.id === currentStep ? "active" : "future";

            return (
              <li key={step.id} className="stepper-item">
                <button
                  type="button"
                  className={`stepper-button stepper-${status}`}
                  onClick={() => onStepSelect(step.id)}
                  aria-current={step.id === currentStep ? "step" : undefined}
                  aria-label={`Step ${step.id}: ${step.title}`}
                >
                  {status === "completed" ? <Check size={14} aria-hidden="true" /> : step.id}
                </button>
                <span className="step-label">{step.label}</span>
                {index < steps.length - 1 ? <span className="stepper-line" aria-hidden="true" /> : null}
              </li>
            );
          })}
        </ol>
      </nav>

      <button type="button" className="help-button" aria-label="Help">
        <HelpCircle size={18} aria-hidden="true" />
        <span>Help</span>
      </button>
    </header>
  );
}

export default AppHeader;
