import { motion } from "framer-motion";
import { useEffect, useRef } from "react";

import { useExplorer } from "./ExplorerContext";

type ContextMenuProps = {
  nodeId: string | null;
  x: number;
  y: number;
  onClose: () => void;
};

export function ContextMenu({ nodeId, x, y, onClose }: ContextMenuProps) {
  const ctx = useExplorer();
  const node = nodeId ? ctx.nodeMap.get(nodeId) : null;
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!nodeId) {
      return;
    }

    const onMouseDown = (event: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(event.target as Node)) {
        onClose();
      }
    };

    document.addEventListener("mousedown", onMouseDown);
    return () => {
      document.removeEventListener("mousedown", onMouseDown);
    };
  }, [nodeId, onClose]);

  if (!node || !nodeId) {
    return null;
  }

  const callerCount = ctx.graph.edges.filter(
    (edge) => edge.relation === "calls" && edge.to === nodeId
  ).length;
  const calleeCount = ctx.graph.edges.filter(
    (edge) => edge.relation === "calls" && edge.from === nodeId
  ).length;

  return (
    <motion.div
      ref={menuRef}
      initial={{ opacity: 0, scale: 0.95 }}
      animate={{ opacity: 1, scale: 1 }}
      exit={{ opacity: 0, scale: 0.95 }}
      transition={{ duration: 0.1, ease: "easeOut" }}
      style={{
        left: x,
        top: y,
        zIndex: 1000,
        transformOrigin: "top left",
      }}
      className="explorer-context-menu"
      role="menu"
      aria-label={`Actions for ${node.label}`}
    >
      <div className="explorer-ctx-menu-header">
        <span className="explorer-ctx-menu-name">{node.label}</span>
        {node.filePath ? <span className="explorer-ctx-menu-path">{node.filePath}</span> : null}
      </div>
      <div className="explorer-ctx-menu-divider" />
      <button
        className="explorer-ctx-menu-item"
        onClick={() => {
          ctx.focusNode(nodeId);
          ctx.showCallers();
          onClose();
        }}
        role="menuitem"
      >
        <span>↑</span> Show callers <span className="explorer-ctx-menu-count">({callerCount})</span>
      </button>
      <button
        className="explorer-ctx-menu-item"
        onClick={() => {
          ctx.focusNode(nodeId);
          ctx.showCallees();
          onClose();
        }}
        role="menuitem"
      >
        <span>↓</span> Show callees <span className="explorer-ctx-menu-count">({calleeCount})</span>
      </button>
      <button
        className="explorer-ctx-menu-item"
        onClick={() => {
          // Wired in Phase 5.
          onClose();
        }}
        role="menuitem"
      >
        <span>⟿</span> Trace to entry
      </button>
      <button
        className="explorer-ctx-menu-item"
        onClick={() => {
          // Wired in Phase 5.
          onClose();
        }}
        role="menuitem"
      >
        <span>⤳</span> Trace dataflow
      </button>
      <div className="explorer-ctx-menu-divider" />
      <button
        className="explorer-ctx-menu-item"
        onClick={() => {
          if (node.filePath) {
            ctx.onNavigateToSource?.(node.filePath, node.line);
          }
          onClose();
        }}
        role="menuitem"
      >
        <span>⌥</span> Open in editor
      </button>
    </motion.div>
  );
}
