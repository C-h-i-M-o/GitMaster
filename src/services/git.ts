import { invoke, isTauri } from "@tauri-apps/api/core";
import type {
  EditableFile,
  LogLevel,
  LogSettings,
  BranchList,
  CloneParent,
  ClonePreview,
  CloneRequest,
  CommitDetail,
  CommitFileList,
  DiffSide,
  FileDiff,
  GitEnvironment,
  HistoryPage,
  ConflictDocument,
  ConflictState,
  ConflictWriteRequest,
  LocalWriteRequest,
  OperationHandle,
  OperationRecord,
  ProjectFileList,
  ProjectTreePage,
  RemoteAssessment,
  SyncTarget,
  RemoteState,
  RemoteWriteRequest,
  RepositoryState,
  UiPreferences,
  WriteContext,
  WritePreview,
} from "../types/git";

import { normalizeOperationError } from "./gitErrors";

/** 文件监视状态，用于在不读取 Git 的情况下判断仓库是否可能变化。 */
export interface RepositoryWatchState {
  repositoryId: string;
  revision: number;
  reliable: boolean;
}

/** 判断当前是否连接桌面 IPC；浏览器预览不发起调用。 */
export const isDesktop = (): boolean => isTauri();
/** 仅解析当前干净分支的上游，不查询服务器或写入配置。 */
export const readSyncTarget = (
  repositoryId: string,
  snapshotId: string,
): Promise<SyncTarget> =>
  call("read_sync_target", { repositoryId, snapshotId });
/** 准备带文件版本的保存任务，执行复用通用一次性计划接口。 */
export const prepareFileSave = (
  repositoryId: string,
  snapshotId: string,
  fileId: string,
  version: string,
  content: string,
): Promise<WritePreview> =>
  call("prepare_file_save", {
    repositoryId,
    snapshotId,
    fileId,
    version,
    content,
  });
/** 读取完整编辑文档和原始文件版本，不复用截断预览。 */
export const readEditableFile = (
  repositoryId: string,
  snapshotId: string,
  fileId: string,
): Promise<EditableFile> =>
  call("read_editable_file", { repositoryId, snapshotId, fileId });

/** 打开已签发仓库的根目录，不向桌面端传入任意路径。 */
export type ExternalAppId = "fileManager" | "vsCode" | "terminal";
export const openProjectFolder = (
  repositoryId: string,
  application: ExternalAppId,
): Promise<void> => call("open_project_folder", { repositoryId, application });
/** 查询支持的外部软件，不运行程序。 */
export const readExternalAvailability = (): Promise<{
  vsCode: boolean;
  terminal: boolean;
}> => call("read_external_availability");

/** 统一处理桌面调用失败并保持错误契约。 */
async function call<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  if (!isTauri())
    throw normalizeOperationError({
      code: "GIT_EXECUTION_FAILED",
      retryable: false,
    });
  try {
    return await invoke<T>(command, args);
  } catch (error: unknown) {
    throw normalizeOperationError(error);
  }
}
/** 调用 Git 环境检测 IPC。 */
export const detectGit = (): Promise<GitEnvironment> => call("detect_git");
/** 验证并保存手动 Git 路径，传 null 恢复自动检测。 */
export const setGitPath = (path: string | null): Promise<GitEnvironment> =>
  call("set_git_path", { path });
/** 打开系统目录选择器并返回选择结果。 */
export const chooseRepositoryPath = (): Promise<string | null> =>
  call("choose_repository_path");
/** 打开 Git 可执行文件选择器。 */
export const chooseGitPath = (): Promise<string | null> =>
  call("choose_git_path");
/** 打开系统 Git 安装页。 */
export const openGitInstallPage = (): Promise<void> =>
  call("open_git_install_page");
/** 打开并读取仓库首个快照。 */
export const openRepository = (path: string): Promise<RepositoryState> =>
  call("open_repository", { path });
/** 刷新当前仓库状态。 */
export const readRepositoryState = (
  repositoryId: string,
): Promise<RepositoryState> => call("read_repository_state", { repositoryId });
/** 读取后端文件监视状态；失败时由调用方降级为低频核实。 */
export const readRepositoryWatch = (
  repositoryId: string,
): Promise<RepositoryWatchState> =>
  call("read_repository_watch", { repositoryId });
/** 读取当前快照中单个文件的差异或预览。 */
export const readFileDiff = (
  repositoryId: string,
  snapshotId: string,
  changeId: string,
  side: DiffSide,
  contextLines = 3,
): Promise<FileDiff> =>
  call("read_file_diff", {
    repositoryId,
    snapshotId,
    changeId,
    side,
    contextLines,
  });

/** 建立历史快照或读取该快照下一页；null 游标明确重新加载图。 */
export const readCommitHistory = (
  repositoryId: string,
  cursor: string | null,
): Promise<HistoryPage> =>
  call("read_commit_history", { repositoryId, cursor });

/** 读取当前图已返回提交的完整身份和说明。 */
export const readCommitDetail = (
  repositoryId: string,
  graphSnapshotId: string,
  oid: string,
): Promise<CommitDetail> =>
  call("read_commit_detail", { repositoryId, graphSnapshotId, oid });

/** 比较已返回提交及其直接父；null 使用第一父或根提交语义。 */
export const readCommitFiles = (
  repositoryId: string,
  graphSnapshotId: string,
  oid: string,
  parentOid: string | null,
): Promise<CommitFileList> =>
  call("read_commit_files", { repositoryId, graphSnapshotId, oid, parentOid });

/** 读取最近提交文件列表签发的差异 ID，不传任意路径。 */
export const readCommitFileDiff = (
  repositoryId: string,
  graphSnapshotId: string,
  fileId: string,
): Promise<FileDiff> =>
  call("read_commit_file_diff", { repositoryId, graphSnapshotId, fileId });

/** 读取真实本地/远端分支与其他工作树占用状态。 */
export const readBranches = (repositoryId: string): Promise<BranchList> =>
  call("read_branches", { repositoryId });

/** 为当前状态建立项目文件列表，忽略文件需用户显式开启。 */
export const readProjectFiles = (
  repositoryId: string,
  snapshotId: string,
  includeIgnored = false,
): Promise<ProjectFileList> =>
  call("read_project_files", { repositoryId, snapshotId, includeIgnored });

/** 首次只获取根目录，子目录在展开时读取。 */
export const readProjectTree = (
  repositoryId: string,
  snapshotId: string,
  includeIgnored: boolean,
): Promise<ProjectTreePage> =>
  call("read_project_tree", { repositoryId, snapshotId, includeIgnored });
/** 目录 ID 和树 ID 双重绑定当前会话，不传文件系统路径。 */
export const readProjectDirectory = (
  repositoryId: string,
  snapshotId: string,
  treeId: string,
  directoryId: string,
  offset: number,
): Promise<ProjectTreePage> =>
  call("read_project_directory", {
    repositoryId,
    snapshotId,
    treeId,
    directoryId,
    offset,
  });
/** 搜索分页复用树中的文件身份，不建立另一组保存能力。 */
export const searchProjectFiles = (
  repositoryId: string,
  snapshotId: string,
  treeId: string,
  query: string,
  offset: number,
): Promise<ProjectTreePage> =>
  call("search_project_files", {
    repositoryId,
    snapshotId,
    treeId,
    query,
    offset,
  });

/** 按后端文件 ID 读取项目文件内容，保持只读预览语义。 */
export const readProjectFile = (
  repositoryId: string,
  snapshotId: string,
  fileId: string,
): Promise<FileDiff> =>
  call("read_project_file", { repositoryId, snapshotId, fileId });

/** 读取当前仓库快照的写入能力上下文。 */
export const readWriteContext = (
  repositoryId: string,
  snapshotId: string,
): Promise<WriteContext> =>
  call("read_write_context", { repositoryId, snapshotId });
/** 预览本地写入操作。 */
export const prepareLocalWrite = (
  repositoryId: string,
  snapshotId: string,
  request: LocalWriteRequest,
): Promise<WritePreview> =>
  call("prepare_local_write", { repositoryId, snapshotId, request });
/** 预览远端写入操作。 */
export const prepareRemoteWrite = (
  repositoryId: string,
  snapshotId: string,
  request: RemoteWriteRequest,
): Promise<WritePreview> =>
  call("prepare_remote_write", { repositoryId, snapshotId, request });
/** 预览冲突写入操作。 */
export const prepareConflictWrite = (
  repositoryId: string,
  mergeSessionId: string,
  request: ConflictWriteRequest,
): Promise<WritePreview> =>
  call("prepare_conflict_write", { repositoryId, mergeSessionId, request });
/** 执行已确认的写入计划。 */
export const executeWrite = (
  repositoryId: string,
  planId: string,
): Promise<OperationHandle> => call("execute_write", { repositoryId, planId });
/** 打开父目录选择器并读取后端签发的目录身份。 */
export const chooseCloneParent = (): Promise<CloneParent | null> =>
  call("choose_clone_parent");
/** 预览仓库克隆操作。 */
export const prepareClone = (request: CloneRequest): Promise<ClonePreview> =>
  call("prepare_clone", { request });
/** 执行已确认的克隆计划。 */
export const executeClone = (planId: string): Promise<OperationHandle> =>
  call("execute_clone", { planId });
/** 查询当前会话中的操作记录；传 null 查询当前任务。 */
export const readOperation = (
  operationId: string | null,
): Promise<OperationRecord | null> => call("read_operation", { operationId });
/** 读取仓库远端状态。 */
export const readRemotes = (repositoryId: string): Promise<RemoteState> =>
  call("read_remotes", { repositoryId });
/** 评估本地快照与远端分支的关系。 */
export const assessRemote = (
  repositoryId: string,
  snapshotId: string,
  remoteBranchId: string,
): Promise<RemoteAssessment> =>
  call("assess_remote", { repositoryId, snapshotId, remoteBranchId });
/** 读取当前仓库冲突状态。 */
export const readConflicts = (repositoryId: string): Promise<ConflictState> =>
  call("read_conflicts", { repositoryId });
/** 读取指定冲突文件的可编辑文档。 */
export const readConflictDocument = (
  repositoryId: string,
  mergeSessionId: string,
  conflictId: string,
): Promise<ConflictDocument> =>
  call("read_conflict_document", { repositoryId, mergeSessionId, conflictId });
/** 读取当前界面偏好。 */
export const readUiPreferences = (): Promise<UiPreferences> =>
  call("read_ui_preferences");
/** 保存并读取校验后的界面偏好。 */
export const setUiPreferences = (
  elasticity: number,
  showLabels: boolean,
): Promise<UiPreferences> =>
  call("set_ui_preferences", { elasticity, showLabels });

/** 读取日志设置。 */
export const readLogSettings = (): Promise<LogSettings> =>
  call("read_log_settings");
/** 保存并立即应用日志级别。 */
export const setLogLevel = (level: LogLevel | null): Promise<LogSettings> =>
  call("set_log_level", { level });
/** 打开固定的应用日志目录。 */
export const openLogDirectory = (): Promise<void> => call("open_log_directory");
