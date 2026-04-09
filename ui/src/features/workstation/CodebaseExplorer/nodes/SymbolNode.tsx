import { Handle, Position } from "reactflow";

import type { FunctionSignature } from "../types";

type SymbolNodeData = {
  label: string;
  kind: string;
  signature?: FunctionSignature;
};

export function SymbolNode({ data }: { data: SymbolNodeData }) {
  return (
    <div className="explorer-symbol-node">
      <Handle type="target" position={Position.Top} style={{ visibility: "hidden" }} />
      <div className="explorer-symbol-name">{data.label}</div>
      {data.signature ? (
        <div className="explorer-symbol-sig">
          <span className="explorer-sig-paren">(</span>
          {data.signature.parameters.map((param, index) => (
            <span key={`${param.name}:${param.position}`}>
              {index > 0 ? <span className="explorer-sig-comma">, </span> : null}
              <span className="explorer-sig-param">
                <span className="explorer-sig-param-name">{param.name}</span>
                {param.typeAnnotation ? (
                  <span className="explorer-sig-param-type">: {param.typeAnnotation}</span>
                ) : null}
              </span>
            </span>
          ))}
          <span className="explorer-sig-paren">)</span>
          {data.signature.returnType ? (
            <span className="explorer-sig-return">
              {" -> "}
              <span className="explorer-sig-return-type">{data.signature.returnType}</span>
            </span>
          ) : null}
        </div>
      ) : null}
      <Handle type="source" position={Position.Bottom} style={{ visibility: "hidden" }} />
    </div>
  );
}
