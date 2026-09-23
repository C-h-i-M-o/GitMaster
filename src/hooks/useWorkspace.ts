import { useCallback, useEffect, useRef, useState } from "react";
import { useGitEnvironment } from "./useGitEnvironment";
import { useRepository } from "./useRepository";
import { useOperations } from "./useOperations";
import { useHistory } from "./useHistory";
import { useRemote } from "./useRemote";
import { useConflicts } from "./useConflicts";
import type { CompletionScope } from "./operationsController";
import {
  chooseGitPath,
  chooseRepositoryPath,
  openGitInstallPage,
  isDesktop,
} from "../services/git";
import { normalizeOperationError } from "../services/gitErrors";
import type {
  ConflictWriteRequest,
  OperationError,
  OperationResult,
} from "../types/git";
/** 组合环境与仓库流程，并处理选择器取消和系统入口失败。 */
export function useWorkspace() {
  const git = useGitEnvironment();
  const repo = useRepository();
  const enabled = isDesktop() && git.environment?.status === "ready";
  // 仓库刷新期间停用旧历史会话，忽略后端返回的过期结果。
  const history = useHistory(repo.repository, enabled && !repo.loading);
  // 首屏历史优先占用仓库队列；失败时仍开放远端入口。
  const remote = useRemote(
    repo.repository,
    enabled && Boolean(history.page || history.error),
  );
  const conflicts = useConflicts(repo.repository, enabled);
  const pendingSave = useRef<{ id: string; content: string } | null>(null);
  const [completion, setCompletion] = useState<{
    result: OperationResult;
    verified: boolean;
  } | null>(null);
  /** 仅当前仓库任务终态刷新真实状态，已核实的 clone 成功后才打开新仓库。 */
  const completed = useCallback(
    async (result: OperationResult, scope: CompletionScope): Promise<void> => {
      setCompletion({ result, verified: scope.verified });
      if (
        result.outcome === "succeeded" &&
        scope.verified &&
        result.kind === "saveConflict" &&
        pendingSave.current
      ) {
        conflicts.acknowledgeSave(
          pendingSave.current.id,
          pendingSave.current.content,
        );
        pendingSave.current = null;
      }
      if (
        result.outcome === "succeeded" &&
        scope.verified &&
        result.kind === "clone" &&
        result.clonePath
      ) {
        await repo.open(result.clonePath);
      } else if (scope.repositoryId === repo.repository?.repositoryId) {
        await repo.refresh();
      }
    },
    [
      repo.open,
      repo.refresh,
      repo.repository?.repositoryId,
      conflicts.acknowledgeSave,
    ],
  );
  const operations = useOperations(repo.repository, enabled, completed);
  useEffect(() => {
    // 焦点变化不能中断正在读取的历史；显式刷新仍由独立入口处理。
    repo.setAutoRefreshBlocked(
      operations.busy ||
        conflicts.dirty ||
        history.loading ||
        history.loadingMore ||
        history.detailLoading ||
        history.filesLoading ||
        history.diffLoading,
    );
  }, [
    repo.setAutoRefreshBlocked,
    operations.busy,
    conflicts.dirty,
    history.loading,
    history.loadingMore,
    history.detailLoading,
    history.filesLoading,
    history.diffLoading,
  ]);
  /** 保存草稿和指纹一起进入确认，失败保留原编辑。 */
  const prepareConflict = useCallback(
    async (session: string, request: ConflictWriteRequest): Promise<void> => {
      if (operations.activity !== "idle") return;
      pendingSave.current =
        request.kind === "saveConflict"
          ? { id: request.conflictId, content: request.content }
          : null;
      await operations.prepareConflict(session, request);
    },
    [operations.activity, operations.prepareConflict],
  );
  /** 手动刷新先关闭预览；活动任务或未保存冲突内容须先处理。 */
  const refreshRepository = useCallback(async (): Promise<void> => {
    if (
      (operations.activity !== "idle" && operations.activity !== "preparing") ||
      conflicts.dirty
    )
      return;
    operations.discardPreview();
    await repo.refresh();
  }, [
    operations.activity,
    operations.discardPreview,
    conflicts.dirty,
    repo.refresh,
  ]);
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
    if (!isDesktop() || choosing || operations.busy || conflicts.dirty) return;
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
  }, [choosing, repo.open, operations.busy, conflicts.dirty]);
  /** 选择并验证 Git；成功保存后作废旧仓库会话。 */
  const chooseGit = useCallback(async (): Promise<void> => {
    if (!isDesktop() || choosing || operations.busy || conflicts.dirty) return;
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
  }, [choosing, git.choose, repo.clear, operations.busy, conflicts.dirty]);
  /** 恢复自动检测也是显式设置操作，成功后清除旧项目。 */
  const resetGit = useCallback(async (): Promise<void> => {
    if (operations.busy || conflicts.dirty) return;
    setActionError(null);
    if (await git.choose(null)) repo.clear();
  }, [git.choose, repo.clear, operations.busy, conflicts.dirty]);
  /** 只打开后端固定官网地址，错误可恢复且不暴露原始异常。 */
  const install = useCallback(async (): Promise<void> => {
    setActionError(null);
    try {
      await openGitInstallPage();
    } catch (error: unknown) {
      setActionError(normalizeOperationError(error));
    }
  }, []);
  /** 显式环境检测同样遵守活动任务与未保存草稿门禁。 */
  const refreshGit = useCallback(async (): Promise<boolean> => {
    if (operations.busy || conflicts.dirty) return false;
    return git.refresh();
  }, [operations.busy, conflicts.dirty, git.refresh]);
  return {
    git: { ...git, refresh: refreshGit },
    repo: { ...repo, refresh: refreshRepository },
    operations: { ...operations, prepareConflict },
    history,
    remote,
    conflicts,
    completion,
    actionError,
    choosing,
    open,
    chooseGit,
    resetGit,
    install,
    canOpen:
      git.environment?.status === "ready" &&
      git.status !== "loading" &&
      !choosing &&
      !operations.busy &&
      !conflicts.dirty,
    preview: !isDesktop(),
  };
}
