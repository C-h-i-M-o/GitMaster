export interface OperationError {
  code: string;
  retryable: boolean;
  diagnostic?: ErrorDiagnostic;
}
export interface ErrorDiagnostic {
  stage: GitDiagnosticStage;
  osCode?: number;
  exitCode?: number;
}
export type GitDiagnosticStage =
  | "revParse"
  | "status"
  | "log"
  | "revList"
  | "forEachRef"
  | "gitQuery"
  | "windowsProcess"
  | "checkoutInit";
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

export type OperationKind =
  | "setUpstream"
  | "saveFile"
  | "stage"
  | "unstage"
  | "commit"
  | "createBranch"
  | "switchBranch"
  | "clone"
  | "fetch"
  | "push"
  | "integrate"
  | "saveConflict"
  | "finishMerge";
export type LocalWriteRequest =
  | { kind: "stage" | "unstage"; changeIds: string[] }
  | { kind: "commit"; message: string }
  | { kind: "createBranch"; name: string }
  | { kind: "switchBranch"; branchId: string };
export type RemoteWriteRequest =
  | { kind: "publishBranch"; remoteId: string; targetBranchName: string }
  | { kind: "setUpstream"; remoteId: string; targetBranchName: string }
  | { kind: "syncPush" }
  | { kind: "syncFastForward" }
  | { kind: "fetchAll"; remoteId: string }
  | { kind: "fetch"; remoteId: string; remoteBranchId: string }
  | { kind: "push"; remoteId: string; targetBranchName: string }
  | {
      kind: "integrate";
      remoteBranchId: string;
      mode: "fastForward" | "merge";
    };
export type ConflictWriteRequest =
  | {
      kind: "saveConflict";
      conflictId: string;
      fingerprint: string;
      content: string;
    }
  | { kind: "finishMerge"; message: string };
export type WriteCapability =
  { status: "allowed" } | { status: "error"; error: OperationError };
export interface WriteContext {
  repositoryId: string;
  snapshotId: string;
  capabilities: {
    stage: WriteCapability;
    unstage: WriteCapability;
    commit: WriteCapability;
    createBranch: WriteCapability;
    switchBranch: WriteCapability;
    fetch: WriteCapability;
    push: WriteCapability;
    integrate: WriteCapability;
    saveConflict: WriteCapability;
    finishMerge: WriteCapability;
  };
}
export type WriteTarget =
  | { kind: "localBranch"; name: string; oid: string | null }
  | {
      kind: "remote";
      remoteId: string;
      displayUrl: string;
      refName: string;
      oid: string | null;
      sourceOid: string | null;
      pendingCommits: CommitSummary[];
    }
  | { kind: "merge"; remoteBranchId: string; oid: string };
export interface WritePreview {
  planId: string;
  repositoryId: string;
  snapshotId: string;
  kind: OperationKind;
  head: HeadState;
  parentOids: string[];
  paths: string[];
  author: string | null;
  message: string | null;
  target: WriteTarget | null;
  warnings: string[];
  expiresAt: number;
}
export interface CloneRequest {
  url: string;
  parentDirectoryId: string;
  directoryName: string;
}
export interface CloneParent {
  parentDirectoryId: string;
  displayPath: string;
}
export interface ClonePreview {
  planId: string;
  displayUrl: string;
  targetPath: string;
  expiresAt: number;
}
export interface OperationHandle {
  operationId: string;
  repositoryId: string | null;
}
export type OperationPhase =
  | "queued"
  | "checking"
  | "transferring"
  | "writing"
  | "verifying"
  | "completed"
  | "failed"
  | "needsResolution"
  | "unknown";
export interface OperationCounts {
  completed: number;
  total: number | null;
}
export interface OperationProgress {
  handle: OperationHandle;
  sequence: number;
  kind: OperationKind;
  phase: OperationPhase;
  counts: OperationCounts | null;
  startedAt: number;
}
export type WriteRefresh =
  | { status: "ready"; state: RepositoryState }
  | { status: "failed"; error: OperationError }
  | { status: "notApplicable" };
export interface ConflictSummary {
  mergeSessionId: string;
  unresolvedCount: number;
  paths: string[];
}
export type CloneStage = "download" | "checkout" | "publish";
export interface CloneRecovery {
  path: string;
  stage: CloneStage;
}
export type OperationResult =
  | {
      outcome: "succeeded";
      operationId: string;
      kind: OperationKind;
      commitOid: string | null;
      branchName: string | null;
      clonePath: string | null;
      refresh: WriteRefresh;
    }
  | {
      outcome: "failed";
      operationId: string;
      kind: OperationKind;
      error: OperationError;
      cloneRecovery: CloneRecovery | null;
      refresh: WriteRefresh;
    }
  | {
      outcome: "unknown";
      operationId: string;
      kind: OperationKind;
      error: OperationError;
      cloneRecovery: CloneRecovery | null;
      refresh: WriteRefresh;
    }
  | {
      outcome: "needsResolution";
      operationId: string;
      kind: OperationKind;
      conflict: ConflictSummary;
      refresh: WriteRefresh;
    };
export interface OperationRecord {
  progress: OperationProgress;
  result: OperationResult | null;
}
export interface UiPreferences {
  elasticity: number;
  showLabels: boolean;
}
export interface CommitSummary {
  oid: string;
  parentOids: string[];
  subject: string;
  authorName: string;
  authoredAt: string;
}
export interface CommitDetail extends CommitSummary {
  body: string;
  authorEmail: string;
  committerName: string;
  committerEmail: string;
  committedAt: string;
  truncated: boolean;
}
export type ReferenceKind = "local" | "remote";
export interface HistoryPage {
  repositoryId: string;
  graphSnapshotId: string;
  tips: Array<{
    refId: string;
    name: string;
    kind: ReferenceKind;
    oid: string;
  }>;
  commits: CommitSummary[];
  nextCursor: string | null;
}
export interface BranchList {
  repositoryId: string;
  branches: Array<{
    branchId: string;
    name: string;
    oid: string;
    kind: ReferenceKind;
    current: boolean;
    occupiedByOtherWorktree: boolean;
    upstreamRefId: string | null;
  }>;
}
export interface CommitFileList {
  repositoryId: string;
  graphSnapshotId: string;
  oid: string;
  parentOid: string | null;
  files: Array<{
    fileId: string;
    path: string;
    originalPath: string | null;
    status: string;
    additions: number | null;
    deletions: number | null;
  }>;
}
export type ProjectFileKind = "tracked" | "untracked" | "ignored";
/** 完整编辑文档与文件身份版本，独立于可截断的预览结果。 */
export interface EditableFile {
  fileId: string;
  path: string;
  version: string;
  text: {
    content: string;
    bom: boolean;
    lineEnding: "none" | "lf" | "crlf" | "cr" | "mixed";
    contentVersion: string;
  };
}
export interface ProjectFileList {
  repositoryId: string;
  snapshotId: string;
  files: Array<{ fileId: string; path: string; kind: ProjectFileKind }>;
}
export interface RemoteState {
  repositoryId: string;
  remotes: Array<{
    remoteId: string;
    name: string;
    fetchDisplayUrl: string;
    fetchedBranchNames: string[] | null;
    pushDisplayUrl: string;
  }>;
  remoteBranches: Array<{ remoteBranchId: string; name: string; oid: string }>;
  branchUpstreams: Array<{
    branchName: string;
    remoteName: string;
    remoteId: string | null;
    targetRef: string;
    trackingRef: string;
  }>;
  lastFetchedAt: string | null;
}
export type RemoteRelation =
  "equal" | "ahead" | "behind" | "diverged" | "unrelated" | "unknown";
export interface RemoteAssessment {
  repositoryId: string;
  snapshotId: string;
  remoteBranchId: string;
  localOid: string;
  remoteOid: string;
  ahead: number;
  behind: number;
  relation: RemoteRelation;
  observedAt: string;
}
/** 同步入口只读解析；configured 仍须联网核验后才能生成写计划。 */
export interface SyncTarget {
  repositoryId: string;
  snapshotId: string;
  branchName: string;
  upstream:
    | { status: "missing" }
    | {
        status: "configured";
        remoteId: string;
        targetBranchName: string;
        remoteBranchId: string | null;
      }
    | {
        status: "unresolved";
        reason:
          | "incomplete"
          | "ambiguous"
          | "localRepository"
          | "unsupportedTarget"
          | "remoteMissing"
          | "mappingMissing";
      };
}
export type ConflictEditorSupport =
  { status: "supported" } | { status: "unsupported"; reason: string };
export interface ConflictState {
  repositoryId: string;
  mergeSessionId: string;
  headOid: string;
  mergeHeadOids: string[];
  files: Array<{
    conflictId: string;
    path: string;
    stageOids: {
      base: string | null;
      local: string | null;
      incoming: string | null;
    };
    editorSupport: ConflictEditorSupport;
  }>;
}
export interface ConflictDocument {
  mergeSessionId: string;
  conflictId: string;
  base: string | null;
  local: string | null;
  incoming: string | null;
  result: string;
  encoding: string;
  lineEnding: string;
  fingerprint: string;
}

/** 日志详细程度与后端实际生效状态。 */
export type LogLevel = "error" | "warn" | "info" | "debug" | "trace";
export interface LogSettings {
  level: LogLevel | null;
  effectiveLevel: LogLevel;
  directory: string;
  available: boolean;
}
