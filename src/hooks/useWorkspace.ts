import { useCallback, useEffect, useState } from "react";
import { useGitEnvironment } from "./useGitEnvironment";
import { useRepository } from "./useRepository";
import {
  chooseGitPath,
  chooseRepositoryPath,
  openGitInstallPage,
  isDesktop,
} from "../services/git";
import { normalizeOperationError } from "../services/gitErrors";
import type { OperationError } from "../types/git";
/** 组合环境与仓库流程，并处理选择器取消和系统入口失败。 */
export function useWorkspace() {
  const git = useGitEnvironment();
  const repo = useRepository();
  const [actionError, setActionError] = useState<OperationError | null>(null);
  const [choosing, setChoosing] = useState(false);
  const identity =
    git.environment?.status === "ready"
      ? `${git.environment.executablePath}\0${git.environment.version}`
      : "";
  useEffect(() => {
    repo.clear();
  }, [identity, repo.clear]);
  /** 使用系统选择器打开工作树，取消不改变已打开项目。 */
  const open = useCallback(async (): Promise<void> => {
    if (!isDesktop() || choosing) return;
    setChoosing(true);
    setActionError(null);
    try {
      const path = await chooseRepositoryPath();
      if (path !== null) await repo.open(path);
    } catch (error: unknown) {
      setActionError(normalizeOperationError(error));
    } finally {
      setChoosing(false);
    }
  }, [choosing, repo.open]);
  /** 选择并验证 Git；成功保存后作废旧仓库会话。 */
  const chooseGit = useCallback(async (): Promise<void> => {
    if (!isDesktop() || choosing) return;
    setChoosing(true);
    setActionError(null);
    try {
      const path = await chooseGitPath();
      if (path !== null && (await git.choose(path))) repo.clear();
    } catch (error: unknown) {
      setActionError(normalizeOperationError(error));
    } finally {
      setChoosing(false);
    }
  }, [choosing, git.choose, repo.clear]);
  /** 恢复自动检测也是显式设置操作，成功后清除旧项目。 */
  const resetGit = useCallback(async (): Promise<void> => {
    setActionError(null);
    if (await git.choose(null)) repo.clear();
  }, [git.choose, repo.clear]);
  /** 只打开后端固定官网地址，错误可恢复且不暴露原始异常。 */
  const install = useCallback(async (): Promise<void> => {
    setActionError(null);
    try {
      await openGitInstallPage();
    } catch (error: unknown) {
      setActionError(normalizeOperationError(error));
    }
  }, []);
  return {
    git,
    repo,
    actionError,
    choosing,
    open,
    chooseGit,
    resetGit,
    install,
    canOpen:
      git.environment?.status === "ready" &&
      git.status !== "loading" &&
      !choosing,
    preview: !isDesktop(),
  };
}
