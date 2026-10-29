import type {
  LocalWriteRequest,
  OperationError,
  RepositoryState,
  RestoreFilePage,
  RestoreSource,
} from "../types/git.ts";
import { normalizeOperationError } from "../services/gitErrors.ts";

export interface RestoreApi {
  repository(): RepositoryState | null;
  blocked(): boolean;
  read(
    id: string,
    snapshot: string,
    source: RestoreSource,
    cursor: string | null,
  ): Promise<RestoreFilePage>;
  prepare(
    request: Extract<LocalWriteRequest, { kind: "restoreFiles" }>,
  ): Promise<void>;
}
interface RestoreState {
  open: boolean;
  busy: boolean;
  source: RestoreSource;
  page: RestoreFilePage | null;
  selected: string[];
  error: OperationError | null;
}

/** 恢复选择绑定快照与来源；关闭、换源和迟到响应均不能产生写请求。 */
export function createRestoreController(api: RestoreApi) {
  let state: RestoreState = {
    open: false,
    busy: false,
    source: { kind: "head" },
    page: null,
    selected: [],
    error: null,
  };
  let generation = 0;
  let owner: { repositoryId: string; snapshotId: string } | null = null;
  const listeners = new Set<() => void>();
  /** 以不可变快照通知 React，避免读取过程中的可见半状态。 */
  function update(patch: Partial<RestoreState>): void {
    state = { ...state, ...patch };
    listeners.forEach((listener) => listener());
  }
  /** 读取和准备均核对当前真实界面快照，不沿用过期文档权限。 */
  function current(): boolean {
    const repo = api.repository();
    return Boolean(
      repo &&
      owner &&
      repo.repositoryId === owner.repositoryId &&
      repo.snapshotId === owner.snapshotId,
    );
  }
  /** 关闭会废弃读取代次，已有后端写任务由通用任务控制器持有。 */
  function close(): void {
    generation += 1;
    owner = null;
    update({ open: false, busy: false, page: null, selected: [], error: null });
  }
  /** 改来源立即取消旧选择，输入本身不触发任何 Git 写入。 */
  function selectSource(source: RestoreSource): void {
    generation += 1;
    owner = null;
    update({ source, busy: false, page: null, selected: [], error: null });
  }
  /** 加载一页候选；后续页不能切换后端来源身份或仓库快照。 */
  async function load(next = false): Promise<void> {
    const repo = api.repository();
    if (!repo || api.blocked() || state.busy || !state.open) return;
    const prior = state.page;
    if (next && (!prior?.nextCursor || !current())) return;
    const token = ++generation;
    owner = { repositoryId: repo.repositoryId, snapshotId: repo.snapshotId };
    const source = state.source;
    update({
      busy: true,
      error: null,
      ...(next ? {} : { page: null, selected: [] }),
    });
    try {
      const page = await api.read(
        repo.repositoryId,
        repo.snapshotId,
        source,
        next ? prior!.nextCursor : null,
      );
      if (token !== generation || !state.open) return;
      if (!current() || (next && page.sourceId !== prior?.sourceId)) {
        update({
          page: null,
          selected: [],
          error: normalizeOperationError({ code: "STALE_REQUEST" }),
        });
        return;
      }
      update({ page });
    } catch (error: unknown) {
      if (token === generation)
        update({ error: normalizeOperationError(error) });
    } finally {
      if (token === generation) update({ busy: false });
    }
  }
  /** 打开默认 HEAD 来源，保持用户主动进入与后续覆盖确认分离。 */
  async function open(): Promise<void> {
    if (api.blocked() || !api.repository()) return;
    selectSource({ kind: "head" });
    update({ open: true });
    await load();
  }
  /** 勾选只允许当前已显示页面签发的文件身份。 */
  function toggle(id: string): void {
    if (state.busy || !state.page?.files.some((file) => file.fileId === id))
      return;
    update({
      selected: state.selected.includes(id)
        ? state.selected.filter((value) => value !== id)
        : [...state.selected, id],
    });
  }
  /** 只进入覆盖预览，必须由通用确认框再次显式执行。 */
  async function prepare(): Promise<void> {
    if (
      state.busy ||
      !state.open ||
      !state.page ||
      !state.selected.length ||
      api.blocked()
    )
      return;
    if (!current()) {
      update({ error: normalizeOperationError({ code: "STALE_REQUEST" }) });
      return;
    }
    const request: Extract<LocalWriteRequest, { kind: "restoreFiles" }> = {
      kind: "restoreFiles",
      source: state.source,
      fileIds: [...state.selected],
    };
    close();
    await api.prepare(request);
  }
  return {
    /** 订阅稳定快照，注销只清理当前监听。 */
    subscribe(listener: () => void): () => void {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    /** 返回当前候选和选择状态。 */
    getSnapshot(): RestoreState {
      return state;
    },
    open,
    close,
    selectSource,
    load,
    toggle,
    prepare,
  };
}
