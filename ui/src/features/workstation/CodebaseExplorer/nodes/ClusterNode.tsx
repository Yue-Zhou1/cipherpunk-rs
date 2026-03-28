import { Handle, Position, type NodeProps } from "reactflow";

import { useExplorer } from "../ExplorerContext";

type ClusterNodeData = {
  label: string;
  childCount: number;
  expanded: boolean;
  kind: "crate" | "module";
};

export function ClusterNode({ id, data }: NodeProps<ClusterNodeData>) {
  const { loadingClusters } = useExplorer();
  const isExpanding = loadingClusters.has(id);

  return (
    <div className="explorer-cluster-node">
      <Handle type="target" position={Position.Top} style={{ visibility: "hidden" }} />
      <div className="explorer-cluster-header">
        <span className="explorer-cluster-label">{data.label}</span>
        <span className="explorer-cluster-count">{data.childCount}</span>
        {isExpanding ? (
          <span
            className="explorer-spinner"
            style={{ width: 14, height: 14 }}
            aria-label="Loading cluster"
          />
        ) : (
          <span
            className="explorer-cluster-toggle"
            aria-label={data.expanded ? "collapse" : "expand"}
          >
            {data.expanded ? "▾" : "▸"}
          </span>
        )}
      </div>
      <Handle type="source" position={Position.Bottom} style={{ visibility: "hidden" }} />
    </div>
  );
}
