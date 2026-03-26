import { Handle, Position } from "reactflow";

type FileNodeData = {
  label: string;
  language?: string;
};

export function FileNode({ data }: { data: FileNodeData }) {
  return (
    <div className="explorer-file-node">
      <Handle type="target" position={Position.Top} style={{ visibility: "hidden" }} />
      <span className="explorer-file-label">{data.label}</span>
      <Handle type="source" position={Position.Bottom} style={{ visibility: "hidden" }} />
    </div>
  );
}
