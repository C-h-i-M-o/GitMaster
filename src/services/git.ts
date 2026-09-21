import { invoke, isTauri } from "@tauri-apps/api/core";
import type {
  DiffSide,
  FileDiff,
  GitEnvironment,
  RepositoryState,
} from "../types/git";

import { normalizeOperationError } from "./gitErrors";

/** 判断当前是否连接桌面 IPC；浏览器预览不发起调用。 */
export const isDesktop = (): boolean => isTauri();

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
/** 读取当前快照中单个文件的差异或预览。 */
export const readFileDiff = (
  repositoryId: string,
  snapshotId: string,
  changeId: string,
  side: DiffSide,
): Promise<FileDiff> =>
  call("read_file_diff", { repositoryId, snapshotId, changeId, side });
