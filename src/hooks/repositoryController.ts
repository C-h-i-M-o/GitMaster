import type {
  DiffSide,
  OperationError,
  RepositoryState,
} from "../types/git.ts";
import type { LocalDiff } from "../types/diff.ts";
import { normalizeOperationError } from "../services/gitErrors.ts";
export interface RepositoryViewState {
  repository: RepositoryState | null;
  loading: boolean;
  stale: boolean;
  error: OperationError | null;
  diff: LocalDiff | null;
  diffLoading: boolean;
  selected: { changeId: string; side: DiffSide; contextLines?: number } | null;
}
export interface RepositoryApi {
  openRepository: (path: string) => Promise<RepositoryState>;
  readRepositoryState: (id: string) => Promise<RepositoryState>;
  readFileDiff: (
    id: string,
    snapshot: string,
    change: string,
    side: DiffSide,
    contextLines?: number,
  ) => Promise<LocalDiff>;
  closeFileDiff: (
    id: string,
    snapshot: string,
    documentId: string,
  ) => Promise<void>;
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
  let automaticVisible = true;
  let automaticFailure = false;
  let dirty = false;
  const listeners = new Set<() => void>();
  /** 发布一个完整快照供视图订阅。 */
  function update(next: Partial<RepositoryViewState>): void {
    state = { ...state, ...next };
    for (const listener of listeners) listener();
  }
  /** 仅分页结果持有后端能力，关闭迟到结果不会清除较新的文档槽。 */
  function release(diff = state.diff, repository = state.repository): void {
    if (diff?.kind === "paged" && repository)
      void api
        .closeFileDiff(
          repository.repositoryId,
          repository.snapshotId,
          diff.documentId,
        )
        .catch(() => {});
  }
  /** 关闭详情同时取消在途选择，防止隐藏面板继续保留正文。 */
  function closeDiff(): void {
    diffRequest++;
    release();
    update({ diff: null, diffLoading: false, selected: null });
  }
  /** 开始读取仓库并丢弃旧差异，不让差异请求使仓库请求失效。 */
  async function load(task: () => Promise<RepositoryState>): Promise<void> {
    release();
    const token = ++repositoryRequest;
    diffRequest += 1;
    pending = true;
    dirty = false;
    automaticFailure = false;
    let succeeded = false;
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
      if (active && token === repositoryRequest) succeeded = true;
    } catch (error: unknown) {
      if (active && token === repositoryRequest) {
        dirty = true;
        automaticFailure = true;
      }
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
        } else if (succeeded && dirty && !autoRefreshBlocked) {
          void refreshAutomatic();
        }
      }
    }
  }
  /** 打开新目录使之前刷新和差异失效。 */
  function open(path: string): Promise<void> {
    queued = false;
    dirty = false;
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
  /** 标记当前仓库可能发生变化，供文件监视事件驱动自动刷新。 */
  function markDirty(): void {
    if (active && state.repository) {
      dirty = true;
      automaticFailure = false;
    }
  }
  /** 操作确认或未保存编辑期间暂停窗口激活刷新。 */
  function setAutoRefreshBlocked(blocked: boolean): void {
    autoRefreshBlocked = blocked;
    if (!blocked) void refreshAutomatic();
  }
  /** 自动刷新遵守界面门禁，显式终态刷新仍使用 refresh。 */
  function refreshAutomatic(): Promise<void> {
    return automaticFailure ||
      !automaticVisible ||
      autoRefreshBlocked ||
      pending ||
      !dirty
      ? Promise.resolve()
      : refresh();
  }
  /** 后台窗口保留 dirty，但不启动自动 Git 查询。 */
  function setAutomaticVisible(visible: boolean): void {
    automaticVisible = visible;
  }
  /** 文件选择使用独立代次，并绑定当前仓库读取代次。 */
  async function selectDiff(
    changeId: string,
    side: DiffSide,
    contextLines = 3,
  ): Promise<void> {
    const repository = state.repository;
    if (!repository || pending || !active) return;
    const token = ++diffRequest;
    const owner = repositoryRequest;
    release();
    update({
      diffLoading: true,
      diff: null,
      selected: {
        changeId,
        side,
        ...(contextLines === 3 ? {} : { contextLines }),
      },
      error: null,
    });
    try {
      const diff = await api.readFileDiff(
        repository.repositoryId,
        repository.snapshotId,
        changeId,
        side,
        contextLines,
      );
      if (active && token === diffRequest && owner === repositoryRequest)
        update({ diff, diffLoading: false });
      else release(diff, repository);
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
    release();
    repositoryRequest += 1;
    diffRequest += 1;
    pending = false;
    queued = false;
    dirty = false;
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
    closeDiff();
    active = false;
    repositoryRequest += 1;
    diffRequest += 1;
    pending = false;
    queued = false;
    dirty = false;
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
    setAutomaticVisible,
    setAutoRefreshBlocked,
    markDirty,
    selectDiff,
    closeDiff,
    clear,
    activate,
    deactivate,
    getSnapshot,
    subscribe,
  };
}
