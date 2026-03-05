import { ChevronLeft, ChevronRight } from "lucide-react";

type FooterNavProps = {
  onBack: () => void;
  onNext: () => void;
  canBack: boolean;
  canNext: boolean;
};

function FooterNav({ onBack, onNext, canBack, canNext }: FooterNavProps): JSX.Element {
  return (
    <footer className="footer-bar" role="contentinfo">
      <button
        type="button"
        className="nav-button nav-button-ghost"
        onClick={onBack}
        disabled={!canBack}
      >
        <ChevronLeft size={16} aria-hidden="true" />
        Back
      </button>
      <button
        type="button"
        className="nav-button nav-button-primary"
        onClick={onNext}
        disabled={!canNext}
      >
        Next Step
        <ChevronRight size={16} aria-hidden="true" />
      </button>
    </footer>
  );
}

export default FooterNav;
