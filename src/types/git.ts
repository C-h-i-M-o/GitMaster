export interface OperationError {
  code: string;
  retryable: boolean;
}
export type GitEnvironment =
  | {
      status: "ready";
      executablePath: string;
      version: string;
      source: "manual" | "path" | "common";
    }
  | { status: "unavailable"; error: OperationError };
export type HeadState =
  | { kind: "branch"; name: string; oid: string }
  | { kind: "unborn"; name: string }
  | { kind: "detached"; oid: string };
export interface FileChange {
  changeId: string;
  path: string;
  originalPath: string | null;
  indexStatus: string;
  worktreeStatus: string;
  kind: "tracked" | "untracked" | "conflicted" | "submodule";
  binary: "unknown" | "text" | "binary";
}
export interface RepositoryState {
  repositoryId: string;
  snapshotId: string;
  rootPath: string;
  head: HeadState;
  operations: Array<"merge" | "rebase" | "cherryPick" | "revert" | "bisect">;
  changes: FileChange[];
}
export type DiffSide = "staged" | "unstaged" | "untracked";
export type FileDiff =
  | { kind: "text"; content: string; truncated: boolean }
  | { kind: "binary" }
  | {
      kind: "unsupported";
      reason: "conflict" | "submodule" | "encoding" | "symlink";
    };
