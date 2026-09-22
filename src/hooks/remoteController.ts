import type {
  OperationError,
  RemoteAssessment,
  RemoteState,
  RepositoryState,
} from "../types/git.ts";
import { normalizeOperationError } from "../services/gitErrors.ts";
export interface RemoteApi {
  readRemotes(repositoryId: string): Promise<RemoteState>;
  assessRemote(
    repositoryId: string,
    snapshotId: string,
    branchId: string,
  ): Promise<RemoteAssessment>;
}
export interface RemoteViewState {
  remote: RemoteState | null;
  assessment: RemoteAssessment | null;
  selectedBranchId: string | null;
  loading: boolean;
  assessing: boolean;
  error: OperationError | null;
}
/** 远端浏览仅读取本地配置及跟踪引用，网络动作交由确认控制器。 */
export function createRemoteController(api: RemoteApi) {
  let state: RemoteViewState = {
    remote: null,
    assessment: null,
    selectedBranchId: null,
    loading: false,
    assessing: false,
    error: null,
  };
  let repository: RepositoryState | null = null;
  let generation = 0;
  let assessmentRequest = 0;
  let active = true;
  const listeners = new Set<() => void>();
  /** 发布不可变状态并通知订阅者。 */
  function update(patch: Partial<RemoteViewState>): void {
    state = { ...state, ...patch };
    if (active) for (const listener of listeners) listener();
  }
  /** 仓库刷新使远端 ID 和评估全部失效。 */
  function setRepository(next: RepositoryState | null): void {
    if (
      repository?.repositoryId === next?.repositoryId &&
      repository?.snapshotId === next?.snapshotId
    )
      return;
    repository = next;
    generation += 1;
    assessmentRequest += 1;
    update({
      remote: null,
      assessment: null,
      selectedBranchId: null,
      loading: false,
      assessing: false,
      error: null,
    });
  }
  /** 显式刷新远端映射，不执行 fetch。 */
  async function refresh(): Promise<void> {
    if (!active || !repository || state.loading) return;
    const id = repository.repositoryId;
    const token = ++generation;
    assessmentRequest += 1;
    update({
      remote: null,
      assessment: null,
      selectedBranchId: null,
      loading: true,
      assessing: false,
      error: null,
    });
    try {
      const remote = await api.readRemotes(id);
      if (!active || token !== generation) return;
      if (remote.repositoryId !== id) throw { code: "STALE_REQUEST" };
      update({ remote, loading: false });
    } catch (error: unknown) {
      if (active && token === generation)
        update({ loading: false, error: normalizeOperationError(error) });
    }
  }
  /** 只评估当前列表中的分支，选择改变立即清除旧关系。 */
  async function selectBranch(branchId: string): Promise<void> {
    if (!active) return;
    if (!branchId) {
      assessmentRequest += 1;
      update({
        selectedBranchId: null,
        assessment: null,
        assessing: false,
        error: null,
      });
      return;
    }
    if (
      !active ||
      !repository ||
      state.loading ||
      !state.remote?.remoteBranches.some(
        (branch) => branch.remoteBranchId === branchId,
      )
    )
      return;
    const repo = repository;
    const token = generation;
    const request = ++assessmentRequest;
    update({
      selectedBranchId: branchId,
      assessment: null,
      assessing: true,
      error: null,
    });
    try {
      const assessment = await api.assessRemote(
        repo.repositoryId,
        repo.snapshotId,
        branchId,
      );
      if (!active || token !== generation || request !== assessmentRequest)
        return;
      if (
        assessment.repositoryId !== repo.repositoryId ||
        assessment.snapshotId !== repo.snapshotId ||
        assessment.remoteBranchId !== branchId
      )
        throw { code: "STALE_REQUEST" };
      update({ assessment, assessing: false });
    } catch (error: unknown) {
      if (active && token === generation && request === assessmentRequest)
        update({ assessing: false, error: normalizeOperationError(error) });
    }
  }
  /** 重新启用订阅。 */
  function activate(): void {
    active = true;
  }
  /** 卸载拒绝在途读取，不改变真实远端。 */
  function deactivate(): void {
    active = false;
    generation += 1;
    assessmentRequest += 1;
    update({ loading: false, assessing: false });
  }
  /** 返回当前稳定状态引用。 */
  function getSnapshot(): RemoteViewState {
    return state;
  }
  /** 注册观察者并提供清理函数。 */
  function subscribe(listener: () => void): () => void {
    listeners.add(listener);
    return () => {
      listeners.delete(listener);
    };
  }
  return {
    setRepository,
    refresh,
    selectBranch,
    activate,
    deactivate,
    getSnapshot,
    subscribe,
  };
}
