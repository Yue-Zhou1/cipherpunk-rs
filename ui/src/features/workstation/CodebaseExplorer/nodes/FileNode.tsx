import { Handle, Position } from "reactflow";

type FileNodeData = {
  label: string;
  childCount?: number;
};

export function FileNode({ data }: { data: FileNodeData }) {
  return (
    <div className="explorer-file-node">
      <Handle type="target" position={Position.Top} style={{ visibility: "hidden" }} />
      <span className="explorer-file-label">{data.label}</span>
      {data.childCount != null && data.childCount > 0 ? (
        <span className="explorer-file-count">{data.childCount}</span>
      ) : null}
      <Handle type="source" position={Position.Bottom} style={{ visibility: "hidden" }} />
    </div>
  );
}
