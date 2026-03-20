import { useEffect } from "react";
import Editor from "@monaco-editor/react";
import type { editor as MonacoEditor } from "monaco-editor";

type CodeEditorPaneProps = {
  filePath: string | null;
  content: string;
  isLoading: boolean;
  error: string | null;
  focusedRecordId?: string | null;
  focusedNodeCount?: number;
  onSymbolFocus?: (symbol: string | null) => void;
};

function editorLanguage(filePath: string | null): string {
  if (!filePath) {
    return "plaintext";
  }

  if (filePath.endsWith(".rs")) {
    return "rust";
  }

  if (filePath.endsWith(".ts") || filePath.endsWith(".tsx")) {
    return "typescript";
  }

  if (filePath.endsWith(".js") || filePath.endsWith(".jsx")) {
    return "javascript";
  }

  if (filePath.endsWith(".md")) {
    return "markdown";
  }

  if (filePath.endsWith(".json")) {
    return "json";
  }

  if (filePath.endsWith(".toml")) {
    return "toml";
  }

  return "plaintext";
}

function shouldRenderMonaco(): boolean {
  if (typeof navigator === "undefined") {
    return false;
  }

  return !/jsdom/i.test(navigator.userAgent);
}

function CodeEditorPane({
  filePath,
  content,
  isLoading,
  error,
  focusedRecordId,
  focusedNodeCount = 0,
  onSymbolFocus,
}: CodeEditorPaneProps): JSX.Element {
  const language = editorLanguage(filePath);

  useEffect(() => {
    onSymbolFocus?.(null);
  }, [filePath, onSymbolFocus]);

  return (
    <section className="panel workstation-editor" aria-label="Code Editor">
      <div className="workstation-panel-head">
        <p className="workstation-panel-eyebrow">Editor</p>
      </div>
      <div className="code-toolbar">
        <h2>Code Editor</h2>
        <span className="muted-text">{filePath ?? "Select a file"}</span>
      </div>
      {focusedRecordId ? (
        <p className="muted-text">
          Focused by review item {focusedRecordId} ({focusedNodeCount} graph node
          {focusedNodeCount === 1 ? "" : "s"}).
        </p>
      ) : null}

      {isLoading ? <p className="muted-text">Loading file...</p> : null}
      {error ? <p className="banner banner-error">{error}</p> : null}

      {!isLoading && !error && filePath ? (
        shouldRenderMonaco() ? (
          <div className="workstation-monaco-wrap">
            <Editor
              theme="vs-dark"
              language={language}
              value={content}
              options={{
                readOnly: true,
                minimap: { enabled: false },
                scrollBeyondLastLine: false,
                automaticLayout: true,
                wordWrap: "on",
                fontSize: 13,
              }}
              onMount={(editorInstance: MonacoEditor.IStandaloneCodeEditor) => {
                editorInstance.onMouseDown((event) => {
                  if (!event.target.position || !onSymbolFocus) {
                    return;
                  }
                  const symbol = editorInstance
                    .getModel()
                    ?.getWordAtPosition(event.target.position)?.word;
                  onSymbolFocus(symbol ?? null);
                });
              }}
            />
          </div>
        ) : (
          <pre className="workstation-editor-fallback" role="region" aria-label="Code content">
            <code>{content}</code>
          </pre>
        )
      ) : null}

      {!isLoading && !error && !filePath ? (
        <p className="muted-text">Choose a file from Project Explorer to view source.</p>
      ) : null}
    </section>
  );
}

export default CodeEditorPane;
