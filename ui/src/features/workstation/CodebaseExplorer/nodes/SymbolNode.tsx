import { Handle, Position } from "reactflow";

import type { FunctionSignature } from "../types";

type SymbolNodeData = {
  label: string;
  signature?: FunctionSignature;
  onParameterClick: (parameterName: string) => void;
  onReturnClick: () => void;
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
              <span
                className="explorer-sig-param"
                role="button"
                tabIndex={0}
                onClick={(event) => {
                  event.stopPropagation();
                  data.onParameterClick(param.name);
                }}
                onKeyDown={(event) => {
                  if (event.key === "Enter") {
                    event.preventDefault();
                    data.onParameterClick(param.name);
                  }
                }}
              >
                <span className="explorer-sig-param-name">{param.name}</span>
                {param.typeAnnotation ? (
                  <span className="explorer-sig-param-type">: {param.typeAnnotation}</span>
                ) : null}
              </span>
            </span>
          ))}
          <span className="explorer-sig-paren">)</span>
          {data.signature.returnType ? (
            <span
              className="explorer-sig-return"
              role="button"
              tabIndex={0}
              onClick={(event) => {
                event.stopPropagation();
                data.onReturnClick();
              }}
              onKeyDown={(event) => {
                if (event.key === "Enter") {
                  event.preventDefault();
                  data.onReturnClick();
                }
              }}
            >
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
