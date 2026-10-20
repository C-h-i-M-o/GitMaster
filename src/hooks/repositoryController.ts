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
  selected: {
    changeId: string;
    side: DiffSide;
    contextLines?: number;
    path?: string;
  } | null;
}
export interface RepositoryApi {
  openRepository: (path: string) => Promise<RepositoryState>;
  readRepositoryState: (
    id: string,
    force?: boolean,
  ) => Promise<RepositoryState>;
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
  let queuedForce = false;
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
  async function load(
    task: () => Promise<RepositoryState>,
    preserveDiff = false,
  ): Promise<void> {
    const previousSelection =
      preserveDiff && state.repository && state.selected
        ? {
            changeId: state.selected.changeId,
            path:
              state.repository.changes.find(
                (item) => item.changeId === state.selected!.changeId,
              )?.path ??
              state.selected.path ??
              null,
            side: state.selected.side,
            contextLines: state.selected.contextLines,
          }
        : null;
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
      ...(preserveDiff
        ? { diff: null, diffLoading: true }
        : { diff: null, diffLoading: false, selected: null }),
    });
    try {
      const repository = await task();
      if (active && token === repositoryRequest) {
        const replacement = previousSelection?.path
          ? repository.changes.find(
              (item) => item.path === previousSelection.path,
            )
          : null;
        update({
          repository,
          loading: false,
          stale: false,
          diffLoading: false,
          diff:
            !replacement && previousSelection?.path
              ? { kind: "text", content: "", truncated: false }
              : null,
          selected: replacement
            ? {
                changeId: replacement.changeId,
                side: previousSelection!.side,
                path: replacement.path,
                ...(previousSelection!.contextLines === undefined
                  ? {}
                  : { contextLines: previousSelection!.contextLines }),
              }
            : previousSelection?.path
              ? {
                  changeId: previousSelection.changeId,
                  side: previousSelection.side,
                  path: previousSelection.path,
                }
              : null,
        });
        // 新快照已经安装，解除在途门禁后才能启动其差异读取。
        pending = false;
        if (replacement)
          void selectDiff(
            replacement.changeId,
            previousSelection!.side,
            previousSelection!.contextLines ?? 1_000_000_000,
          );
      }
      if (active && token === repositoryRequest) succeeded = true;
    } catch (error: unknown) {
      if (active && token === repositoryRequest) {
        dirty = true;
        automaticFailure = true;
      }
      if (active && token === repositoryRequest) {
        const normalized = normalizeOperationError(error);
        if (normalized.code === "REQUEST_CANCELLED") {
          dirty = true;
          automaticFailure = false;
          update({
            loading: false,
            stale: state.repository !== null,
            diffLoading: false,
          });
        } else
          update({
            loading: false,
            stale: state.repository !== null,
            error: normalized,
            diffLoading: false,
          });
      }
    } finally {
      if (active && token === repositoryRequest) {
        pending = false;
        if (queued) {
          queued = false;
          const force = queuedForce;
          queuedForce = false;
          void refresh(force);
        } else if (succeeded && dirty && !autoRefreshBlocked) {
          void refreshAutomatic();
        }
      }
    }
  }
  /** 打开新目录使之前刷新和差异失效。 */
  function open(path: string): Promise<void> {
    queued = false;
    queuedForce = false;
    dirty = false;
    return load(() => api.openRepository(path));
  }
  /** 在途刷新合并为至多一次后续刷新。 */
  function refresh(force = true): Promise<void> {
    if (!active || !state.repository) return Promise.resolve();
    if (pending) {
      queued = true;
      queuedForce ||= force;
      return Promise.resolve();
    }
    const id = state.repository.repositoryId;
    return load(() => api.readRepositoryState(id, force), true);
  }
  /** 消费桌面端同代次核实快照，拒绝迟到结果并保留刷新降级路径。 */
  function consumeVerifiedState(
    repository: RepositoryState,
    expectedSnapshotId = state.repository?.snapshotId,
  ): boolean {
    if (
      !active ||
      !state.repository ||
      pending ||
      state.repository.snapshotId !== expectedSnapshotId ||
      state.repository.repositoryId !== repository.repositoryId
    )
      return false;
    repositoryRequest += 1;
    diffRequest += 1;
    release();
    update({
      repository,
      loading: false,
      stale: false,
      error: null,
      diff: null,
      diffLoading: false,
      selected: null,
    });
    automaticFailure = false;
    return true;
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
    return automaticFailure || !automaticVisible || autoRefreshBlocked || !dirty
      ? Promise.resolve()
      : refresh(false);
  }
  /** 后台窗口保留 dirty，但不启动自动 Git 查询。 */
  function setAutomaticVisible(visible: boolean): void {
    automaticVisible = visible;
  }
  /** 文件选择使用独立代次，并绑定当前仓库读取代次。 */
  async function selectDiff(
    changeId: string,
    side: DiffSide,
    contextLines = 1_000_000_000,
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
        ...(repository.changes.find((item) => item.changeId === changeId)?.path
          ? {
              path: repository.changes.find(
                (item) => item.changeId === changeId,
              )!.path,
            }
          : {}),
        ...(contextLines === 1_000_000_000 ? {} : { contextLines }),
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
        if (normalized.code === "REQUEST_CANCELLED") {
          dirty = true;
          automaticFailure = false;
          update({ diffLoading: false, error: null });
          return;
        }
        if (normalized.code === "STALE_REQUEST") {
          dirty = true;
          automaticFailure = false;
          update({ diffLoading: false, error: null });
          void refreshAutomatic();
          return;
        }
        update({
          diffLoading: false,
          error: normalized,
          stale: state.stale,
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
    queuedForce = false;
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
    queuedForce = false;
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
    consumeVerifiedState,
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
