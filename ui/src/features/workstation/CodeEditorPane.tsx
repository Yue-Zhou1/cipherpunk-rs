import { useEffect, useState } from "react";
import Editor from "@monaco-editor/react";
import type { editor as MonacoEditor } from "monaco-editor";

type CodeEditorPaneProps = {
  filePath: string | null;
  content: string;
  isLoading: boolean;
  error: string | null;
  preferPlainText?: boolean;
  focusedRecordId?: string | null;
  focusedNodeCount?: number;
  onSymbolFocus?: (symbol: string | null) => void;
};

type PlainTextCodeViewerProps = {
  content: string;
  className?: string;
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

function splitPlainTextLines(content: string): string[] {
  const lines = content.replace(/\r\n/g, "\n").split("\n");
  return lines.length > 0 ? lines : [""];
}

function PlainTextCodeViewer({
  content,
  className,
}: PlainTextCodeViewerProps): JSX.Element {
  const lines = splitPlainTextLines(content);
  const classes = [
    "workstation-editor-fallback",
    "workstation-plain-text-viewer",
    className,
  ]
    .filter(Boolean)
    .join(" ");

  return (
    <div className={classes} role="region" aria-label="Code content">
      <ol className="workstation-plain-text-lines">
        {lines.map((line, index) => (
          <li key={`${index}:${line.slice(0, 24)}`} className="workstation-plain-text-line">
            <span className="workstation-plain-text-gutter" data-testid="code-line-number" aria-hidden="true">
              {index + 1}
            </span>
            <code className="workstation-plain-text-code">{line.length === 0 ? " " : line}</code>
          </li>
        ))}
      </ol>
    </div>
  );
}

function CodeEditorPane({
  filePath,
  content,
  isLoading,
  error,
  preferPlainText = false,
  focusedRecordId,
  focusedNodeCount = 0,
  onSymbolFocus,
}: CodeEditorPaneProps): JSX.Element {
  const language = editorLanguage(filePath);
  const [monacoMounted, setMonacoMounted] = useState(false);
  const useMonaco = shouldRenderMonaco() && !preferPlainText;

  useEffect(() => {
    onSymbolFocus?.(null);
  }, [filePath, onSymbolFocus]);

  useEffect(() => {
    setMonacoMounted(false);
  }, [filePath, language]);

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
        useMonaco ? (
          <div className="workstation-monaco-wrap">
            <Editor
              path={filePath ?? undefined}
              theme="vs-dark"
              language={language}
              value={content}
              height="100%"
              width="100%"
              options={{
                readOnly: true,
                minimap: { enabled: false },
                scrollBeyondLastLine: false,
                automaticLayout: true,
                wordWrap: "on",
                fontSize: 13,
              }}
              onMount={(editorInstance: MonacoEditor.IStandaloneCodeEditor) => {
                setMonacoMounted(true);
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
            {!monacoMounted ? (
              <PlainTextCodeViewer
                content={content}
                className="workstation-editor-fallback-overlay"
              />
            ) : null}
          </div>
        ) : (
          <PlainTextCodeViewer content={content} />
        )
      ) : null}

      {!isLoading && !error && !filePath ? (
        <p className="muted-text">Choose a file from Project Explorer to view source.</p>
      ) : null}
    </section>
  );
}

export default CodeEditorPane;
