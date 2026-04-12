import { useEffect, useRef } from "react";

type NodeContextMenuProps = {
  x: number;
  y: number;
  nodeId: string;
  hasFilePath: boolean;
  isFocused: boolean;
  onShowCallers: () => void;
  onShowCallees: () => void;
  onOpenInEditor: () => void;
  onToggleFocus: () => void;
  onClose: () => void;
};

export function NodeContextMenu({
  x,
  y,
  nodeId,
  hasFilePath,
  isFocused,
  onShowCallers,
  onShowCallees,
  onOpenInEditor,
  onToggleFocus,
  onClose,
}: NodeContextMenuProps) {
  const menuRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    const onPointerDown = (event: PointerEvent) => {
      if (!menuRef.current?.contains(event.target as Node)) {
        onClose();
      }
    };

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        onClose();
      }
    };

    window.addEventListener("pointerdown", onPointerDown);
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("pointerdown", onPointerDown);
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [onClose]);

  return (
    <div
      ref={menuRef}
      className="explorer-context-menu"
      style={{ left: x, top: y }}
      role="menu"
      aria-label={`Node actions for ${nodeId}`}
    >
      <button type="button" onClick={onShowCallers}>
        Show callers
      </button>
      <button type="button" onClick={onShowCallees}>
        Show callees
      </button>
      {hasFilePath ? (
        <button type="button" onClick={onOpenInEditor}>
          Open in editor
        </button>
      ) : null}
      <button type="button" onClick={onToggleFocus}>
        {isFocused ? "Unfocus" : "Focus"}
      </button>
    </div>
  );
}
