import { Handle, Position } from "reactflow";

type ClusterNodeData = {
  label: string;
  childCount: number;
  expanded: boolean;
  kind: "crate" | "module";
};

export function ClusterNode({ data }: { data: ClusterNodeData }) {
  return (
    <div className="explorer-cluster-node">
      <Handle type="target" position={Position.Top} style={{ visibility: "hidden" }} />
      <div className="explorer-cluster-header">
        <span className="explorer-cluster-label">{data.label}</span>
        <span className="explorer-cluster-count">{data.childCount}</span>
        <span
          className="explorer-cluster-toggle"
          aria-label={data.expanded ? "collapse" : "expand"}
        >
          {data.expanded ? "▾" : "▸"}
        </span>
      </div>
      <Handle type="source" position={Position.Bottom} style={{ visibility: "hidden" }} />
    </div>
  );
}
