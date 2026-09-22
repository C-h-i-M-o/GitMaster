import type {
  DiffSide,
  FileDiff,
  OperationError,
  RepositoryState,
} from "../types/git.ts";
import { normalizeOperationError } from "../services/gitErrors.ts";
export interface RepositoryViewState {
  repository: RepositoryState | null;
  loading: boolean;
  stale: boolean;
  error: OperationError | null;
  diff: FileDiff | null;
  diffLoading: boolean;
  selected: { changeId: string; side: DiffSide } | null;
}
export interface RepositoryApi {
  openRepository: (path: string) => Promise<RepositoryState>;
  readRepositoryState: (id: string) => Promise<RepositoryState>;
  readFileDiff: (
    id: string,
    snapshot: string,
    change: string,
    side: DiffSide,
  ) => Promise<FileDiff>;
}
/** 构造可测试的仓库状态控制器，窗口生命周期与 React 渲染分离。 */
export function createRepositoryController(api: RepositoryApi) {
  let state: RepositoryViewState = {
    repository: null,
    loading: false,
    stale: false,
    error: null,
    diff: null,
    diffLoading: false,
    selected: null,
  };
  let repositoryRequest = 0;
  let diffRequest = 0;
  let active = true;
  let pending = false;
  let queued = false;
  let autoRefreshBlocked = false;
  const listeners = new Set<() => void>();
  /** 发布一个完整快照供视图订阅。 */
  function update(next: Partial<RepositoryViewState>): void {
    state = { ...state, ...next };
    for (const listener of listeners) listener();
  }
  /** 开始读取仓库并丢弃旧差异，不让差异请求使仓库请求失效。 */
  async function load(task: () => Promise<RepositoryState>): Promise<void> {
    const token = ++repositoryRequest;
    diffRequest += 1;
    pending = true;
    update({
      loading: true,
      error: null,
      diff: null,
      diffLoading: false,
      selected: null,
    });
    try {
      const repository = await task();
      if (active && token === repositoryRequest)
        update({ repository, loading: false, stale: false });
    } catch (error: unknown) {
      if (active && token === repositoryRequest)
        update({
          loading: false,
          stale: state.repository !== null,
          error: normalizeOperationError(error),
        });
    } finally {
      if (active && token === repositoryRequest) {
        pending = false;
        if (queued) {
          queued = false;
          void refresh();
        }
      }
    }
  }
  /** 打开新目录使之前刷新和差异失效。 */
  function open(path: string): Promise<void> {
    queued = false;
    return load(() => api.openRepository(path));
  }
  /** 在途刷新合并为至多一次后续刷新。 */
  function refresh(): Promise<void> {
    if (!active || !state.repository) return Promise.resolve();
    if (pending) {
      queued = true;
      return Promise.resolve();
    }
    const id = state.repository.repositoryId;
    return load(() => api.readRepositoryState(id));
  }
  /** 操作确认或未保存编辑期间暂停窗口激活刷新。 */
  function setAutoRefreshBlocked(blocked: boolean): void {
    autoRefreshBlocked = blocked;
  }
  /** 自动刷新遵守界面门禁，显式终态刷新仍使用 refresh。 */
  function refreshAutomatic(): Promise<void> {
    return autoRefreshBlocked ? Promise.resolve() : refresh();
  }
  /** 文件选择使用独立代次，并绑定当前仓库读取代次。 */
  async function selectDiff(changeId: string, side: DiffSide): Promise<void> {
    const repository = state.repository;
    if (!repository || pending || !active) return;
    const token = ++diffRequest;
    const owner = repositoryRequest;
    update({
      diffLoading: true,
      diff: null,
      selected: { changeId, side },
      error: null,
    });
    try {
      const diff = await api.readFileDiff(
        repository.repositoryId,
        repository.snapshotId,
        changeId,
        side,
      );
      if (active && token === diffRequest && owner === repositoryRequest)
        update({ diff, diffLoading: false });
    } catch (error: unknown) {
      if (active && token === diffRequest && owner === repositoryRequest) {
        const normalized = normalizeOperationError(error);
        update({
          diffLoading: false,
          error: normalized,
          stale: state.stale || normalized.code === "STALE_REQUEST",
        });
      }
    }
  }
  /** 清除当前项目，环境变更后所有在途结果失效。 */
  function clear(): void {
    repositoryRequest += 1;
    diffRequest += 1;
    pending = false;
    queued = false;
    update({
      repository: null,
      loading: false,
      stale: false,
      error: null,
      diff: null,
      diffLoading: false,
      selected: null,
    });
  }
  /** 卸载时废弃响应，严格模式再次挂载可重新激活。 */
  function deactivate(): void {
    active = false;
    repositoryRequest += 1;
    diffRequest += 1;
    pending = false;
    queued = false;
  }
  /** 激活订阅生命周期。 */
  function activate(): void {
    active = true;
  }
  /** 返回不可变快照。 */
  function getSnapshot(): RepositoryViewState {
    return state;
  }
  /** 订阅快照变更并提供解除订阅函数。 */
  function subscribe(listener: () => void): () => void {
    listeners.add(listener);
    return () => {
      listeners.delete(listener);
    };
  }
  return {
    open,
    refresh,
    refreshAutomatic,
    setAutoRefreshBlocked,
    selectDiff,
    clear,
    activate,
    deactivate,
    getSnapshot,
    subscribe,
  };
}
