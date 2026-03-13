import { useCallback, useEffect, useMemo, useState } from "react";

import {
  getProjectTree,
  readSourceFile,
  tailSessionConsole,
  type ProjectTreeNode,
  type SessionConsoleEntry,
} from "../../ipc/commands";

type SessionStateResult = {
  projectTree: ProjectTreeNode[];
  selectedFilePath: string | null;
  fileContent: string;
  consoleEntries: SessionConsoleEntry[];
  treeLoading: boolean;
  fileLoading: boolean;
  treeError: string | null;
  fileError: string | null;
  selectFile: (path: string) => void;
};

function firstFilePath(nodes: ProjectTreeNode[]): string | null {
  for (const node of nodes) {
    if (node.kind === "file") {
      return node.path;
    }

    const nested = firstFilePath(node.children);
    if (nested) {
      return nested;
    }
  }

  return null;
}

function hasFile(nodes: ProjectTreeNode[], path: string): boolean {
  for (const node of nodes) {
    if (node.kind === "file" && node.path === path) {
      return true;
    }

    if (node.children.length > 0 && hasFile(node.children, path)) {
      return true;
    }
  }

  return false;
}

export default function useSessionState(sessionId: string): SessionStateResult {
  const [projectTree, setProjectTree] = useState<ProjectTreeNode[]>([]);
  const [selectedFilePath, setSelectedFilePath] = useState<string | null>(null);
  const [fileContent, setFileContent] = useState("");
  const [consoleEntries, setConsoleEntries] = useState<SessionConsoleEntry[]>([]);
  const [treeLoading, setTreeLoading] = useState(false);
  const [fileLoading, setFileLoading] = useState(false);
  const [treeError, setTreeError] = useState<string | null>(null);
  const [fileError, setFileError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;

    setTreeLoading(true);
    setTreeError(null);

    void getProjectTree(sessionId)
      .then((response) => {
        if (cancelled) {
          return;
        }

        setProjectTree(response.nodes);
        setSelectedFilePath((current) => {
          if (current && hasFile(response.nodes, current)) {
            return current;
          }

          return firstFilePath(response.nodes);
        });
      })
      .catch(() => {
        if (!cancelled) {
          setTreeError("Unable to load project tree.");
          setProjectTree([]);
          setSelectedFilePath(null);
        }
      })
      .finally(() => {
        if (!cancelled) {
          setTreeLoading(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [sessionId]);

  useEffect(() => {
    if (!selectedFilePath) {
      setFileContent("");
      setFileError(null);
      setFileLoading(false);
      return;
    }

    let cancelled = false;
    setFileLoading(true);
    setFileError(null);

    void readSourceFile(sessionId, selectedFilePath)
      .then((response) => {
        if (!cancelled) {
          setFileContent(response.content);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setFileError("Unable to read selected file.");
          setFileContent("");
        }
      })
      .finally(() => {
        if (!cancelled) {
          setFileLoading(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [sessionId, selectedFilePath]);

  const refreshConsole = useCallback(() => {
    void tailSessionConsole(sessionId)
      .then((response) => {
        setConsoleEntries(response.entries);
      })
      .catch(() => {
        setConsoleEntries([]);
      });
  }, [sessionId]);

  useEffect(() => {
    refreshConsole();
    const timer = window.setInterval(refreshConsole, 3000);

    return () => {
      window.clearInterval(timer);
    };
  }, [refreshConsole]);

  const selectFile = useCallback((path: string) => {
    setSelectedFilePath(path);
  }, []);

  return useMemo(
    () => ({
      projectTree,
      selectedFilePath,
      fileContent,
      consoleEntries,
      treeLoading,
      fileLoading,
      treeError,
      fileError,
      selectFile,
    }),
    [
      consoleEntries,
      fileContent,
      fileError,
      fileLoading,
      projectTree,
      selectFile,
      selectedFilePath,
      treeError,
      treeLoading,
    ]
  );
}
