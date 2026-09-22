import type {
  ClonePreview,
  CloneRequest,
  ConflictWriteRequest,
  LocalWriteRequest,
  OperationError,
  OperationHandle,
  OperationKind,
  OperationRecord,
  OperationResult,
  RemoteWriteRequest,
  RepositoryState,
  WritePreview,
} from "../types/git.ts";
import { normalizeOperationError } from "../services/gitErrors.ts";

export interface OperationsApi {
  prepareLocalWrite(
    id: string,
    snapshot: string,
    request: LocalWriteRequest,
  ): Promise<WritePreview>;
  prepareRemoteWrite(
    id: string,
    snapshot: string,
    request: RemoteWriteRequest,
  ): Promise<WritePreview>;
  prepareConflictWrite(
    id: string,
    session: string,
    request: ConflictWriteRequest,
  ): Promise<WritePreview>;
  prepareClone(request: CloneRequest): Promise<ClonePreview>;
  executeWrite(id: string, planId: string): Promise<OperationHandle>;
  executeClone(planId: string): Promise<OperationHandle>;
  readOperation(id: string | null): Promise<OperationRecord | null>;
}
export type Confirmation =
  | { type: "write"; value: WritePreview }
  | { type: "clone"; value: ClonePreview };
export type OperationActivity =
  "idle" | "preparing" | "executing" | "querying" | "running" | "unverified";
export interface OperationEvent {
  operationId: string;
  repositoryId: string | null;
  kind: OperationKind;
  phase: OperationRecord["progress"]["phase"];
  counts: OperationRecord["progress"]["counts"];
  sequence: number;
  observedAt: number;
}
export interface OperationsViewState {
  activity: OperationActivity;
  preview: Confirmation | null;
  handle: OperationHandle | null;
  record: OperationRecord | null;
  error: OperationError | null;
  events: OperationEvent[];
  busy: boolean;
}
export interface CompletionScope {
  repositoryId: string | null;
  verified: boolean;
}
export type CompletionHandler = (
  result: OperationResult,
  scope: CompletionScope,
) => Promise<void>;
/** 调度器返回取消函数，测试可以推进虚拟时间而不依赖真实等待。 */
export type OperationSchedule = (
  callback: () => void,
  delay: number,
) => () => void;
interface Owner extends CompletionScope {
  key: string;
}
interface Recovery {
  baseline: string | null;
  kind: OperationKind;
  owner: Owner;
}
/** 使用平台定时器，仅调度下一次查询，不创建重叠轮询。 */
function schedule(callback: () => void, delay: number): () => void {
  const timer = setTimeout(callback, delay);
  return () => clearTimeout(timer);
}
/** 管理已确认任务；窗口只停止查询，不拥有或取消后台 Git 生命周期。 */
export function createOperationsController(
  api: OperationsApi,
  later: OperationSchedule = schedule,
) {
  let state: OperationsViewState = {
    activity: "idle",
    preview: null,
    handle: null,
    record: null,
    error: null,
    events: [],
    busy: false,
  };
  let repository: RepositoryState | null = null;
  let enabled = false;
  let active = true;
  let key = "";
  let generation = 0;
  let queryGeneration = 0;
  let queryPending = false;
  let cancelTimer: (() => void) | null = null;
  let owner: Owner | null = null;
  let recovery: Recovery | null = null;
  let completedId: string | null = null;
  let onComplete: CompletionHandler | null = null;
  const listeners = new Set<() => void>();
  /** 发布不可变状态；busy 包括未核实任务和当前确认框。 */
  function update(patch: Partial<OperationsViewState>): void {
    state = { ...state, ...patch };
    state = {
      ...state,
      busy: state.activity !== "idle" || state.preview !== null,
    };
    if (active) for (const listener of listeners) listener();
  }
  /** 停止定时器，并使在途读取结果失效。 */
  function stopQuery(): void {
    cancelTimer?.();
    cancelTimer = null;
    queryGeneration += 1;
    queryPending = false;
  }
  /** 只有当前窗口和仓库上下文才能交付准备结果。 */
  function current(token: number): boolean {
    return active && enabled && generation === token;
  }
  /** 上下文变化只作废前端预览，不取消后台任务或替换其身份。 */
  function setRepository(
    next: RepositoryState | null,
    available: boolean,
  ): void {
    const nextKey = available
      ? `${next?.repositoryId ?? ""}\0${next?.snapshotId ?? ""}`
      : "disabled";
    repository = next;
    enabled = available;
    if (nextKey === key) return;
    key = nextKey;
    generation += 1;
    update({
      preview: null,
      ...(state.activity === "preparing" ? { activity: "idle" as const } : {}),
    });
    if (!enabled) stopQuery();
  }
  /** 设置终态处理器；调用方负责显式刷新或打开 clone 结果。 */
  function setCompletionHandler(handler: CompletionHandler): void {
    onComplete = handler;
  }
  /** 重新准备会替换旧确认，重复点击在同步门禁处返回。 */
  async function prepare(task: () => Promise<Confirmation>): Promise<void> {
    if (!active || !enabled || state.activity !== "idle") return;
    const token = ++generation;
    update({ activity: "preparing", preview: null, error: null });
    try {
      const preview = await task();
      if (current(token)) {
        if (
          preview.type === "write" &&
          (preview.value.repositoryId !== repository?.repositoryId ||
            preview.value.snapshotId !== repository?.snapshotId)
        )
          throw { code: "STALE_WRITE_PLAN" };
        update({ activity: "idle", preview });
      }
    } catch (error: unknown) {
      if (current(token))
        update({ activity: "idle", error: normalizeOperationError(error) });
    }
  }
  /** 准备当前快照选定的本地操作。 */
  function prepareLocal(request: LocalWriteRequest): Promise<void> {
    const repo = repository;
    if (!repo) return Promise.resolve();
    return prepare(async () => ({
      type: "write",
      value: await api.prepareLocalWrite(
        repo.repositoryId,
        repo.snapshotId,
        request,
      ),
    }));
  }
  /** 远端准备只能由显式表单动作触发，push 可能查询真实远端。 */
  function prepareRemote(request: RemoteWriteRequest): Promise<void> {
    const repo = repository;
    if (!repo) return Promise.resolve();
    return prepare(async () => ({
      type: "write",
      value: await api.prepareRemoteWrite(
        repo.repositoryId,
        repo.snapshotId,
        request,
      ),
    }));
  }
  /** 冲突保存将文档指纹原样传入当前合并会话。 */
  function prepareConflict(
    session: string,
    request: ConflictWriteRequest,
  ): Promise<void> {
    const repo = repository;
    if (!repo) return Promise.resolve();
    return prepare(async () => ({
      type: "write",
      value: await api.prepareConflictWrite(
        repo.repositoryId,
        session,
        request,
      ),
    }));
  }
  /** clone 无需已有仓库，仍要求已连接桌面 Git 环境。 */
  function prepareClone(request: CloneRequest): Promise<void> {
    return prepare(async () => ({
      type: "clone",
      value: await api.prepareClone(request),
    }));
  }
  /** 关闭确认只丢弃前端预览，不代表任何已启动操作被回滚。 */
  function discardPreview(): void {
    if (state.activity !== "idle" && state.activity !== "preparing") return;
    generation += 1;
    update({ preview: null, activity: "idle", error: null });
  }
  /** 校验响应归属与序号，避免不同任务或倒退进度进入当前视图。 */
  function validRecord(
    record: OperationRecord,
    handle: OperationHandle,
  ): boolean {
    return (
      record.progress.handle.operationId === handle.operationId &&
      record.progress.handle.repositoryId === handle.repositoryId &&
      (!state.record ||
        state.record.progress.handle.operationId !== handle.operationId ||
        record.progress.sequence >= state.record.progress.sequence) &&
      (!record.result ||
        (record.result.operationId === handle.operationId &&
          record.result.kind === record.progress.kind))
    );
  }
  /** 保存有界阶段记录，不保留请求内容、远端原始地址或 stderr。 */
  function appendEvent(record: OperationRecord): OperationEvent[] {
    const p = record.progress;
    const last = state.events.at(-1);
    if (
      last?.operationId === p.handle.operationId &&
      last.sequence === p.sequence &&
      last.phase === p.phase
    )
      return state.events;
    return [
      ...state.events,
      {
        operationId: p.handle.operationId,
        repositoryId: p.handle.repositoryId,
        kind: p.kind,
        phase: p.phase,
        counts: p.counts,
        sequence: p.sequence,
        observedAt: Date.now(),
      },
    ].slice(-200);
  }
  /** 仅本次上下文的终态触发一次后续处理，恢复记录不清除表单输入。 */
  async function accept(
    record: OperationRecord,
    notify: boolean,
  ): Promise<void> {
    const result = record.result;
    update({
      record,
      error: null,
      events: appendEvent(record),
      activity: result ? "idle" : "running",
    });
    if (result) {
      stopQuery();
      recovery = null;
      if (
        notify &&
        owner &&
        owner.key === key &&
        completedId !== result.operationId
      ) {
        completedId = result.operationId;
        const token = generation;
        try {
          await onComplete?.(result, {
            repositoryId: owner.repositoryId,
            verified: owner.verified,
          });
        } catch (error: unknown) {
          if (current(token)) update({ error: normalizeOperationError(error) });
        }
      }
    }
  }
  /** 查询失败保留操作身份，不自动重复执行；用户可仅重试查询。 */
  async function poll(): Promise<void> {
    const handle = state.handle;
    if (!active || !enabled || !handle || state.record?.result || queryPending)
      return;
    cancelTimer?.();
    cancelTimer = null;
    const token = ++queryGeneration;
    queryPending = true;
    try {
      const record = await api.readOperation(handle.operationId);
      if (!active || !enabled || token !== queryGeneration) return;
      if (!record || !validRecord(record, handle))
        throw { code: "STALE_REQUEST", retryable: true };
      await accept(record, true);
      if (active && enabled && token === queryGeneration && !record.result)
        cancelTimer = later(() => {
          void poll();
        }, 500);
    } catch (error: unknown) {
      if (active && enabled && token === queryGeneration)
        update({
          activity: "unverified",
          error: normalizeOperationError(error),
        });
    } finally {
      if (token === queryGeneration) queryPending = false;
    }
  }
  /** 丢失执行响应时查找新任务，旧终态记录不能被当成本次执行成功。 */
  async function recoverExecution(): Promise<void> {
    const pending = recovery;
    if (!pending || queryPending) return;
    const token = ++queryGeneration;
    queryPending = true;
    update({ activity: "querying", error: null });
    try {
      const record = await api.readOperation(null);
      if (!active || !enabled || token !== queryGeneration) return;
      if (
        record &&
        record.progress.handle.operationId !== pending.baseline &&
        record.progress.kind === pending.kind &&
        record.progress.handle.repositoryId === pending.owner.repositoryId
      ) {
        owner = { ...pending.owner, verified: false };
        recovery = null;
        update({ handle: record.progress.handle, record: null, preview: null });
        await accept(record, true);
        if (!record.result && active && enabled)
          cancelTimer = later(() => {
            void poll();
          }, 500);
      } else {
        recovery = null;
        update({
          activity: "idle",
          preview: null,
          error: normalizeOperationError({
            code: "WRITE_OUTCOME_UNKNOWN",
            retryable: false,
          }),
        });
      }
    } catch (error: unknown) {
      if (active && enabled && token === queryGeneration)
        update({
          activity: "unverified",
          error: normalizeOperationError(error),
        });
    } finally {
      if (token === queryGeneration) queryPending = false;
    }
  }
  /** 确认前先记住最近任务，执行入口同步加锁且只调用一次。 */
  async function confirm(): Promise<void> {
    const preview = state.preview;
    if (!active || !enabled || state.activity !== "idle" || !preview) return;
    if (Date.now() >= preview.value.expiresAt) {
      update({
        preview: null,
        error: normalizeOperationError({ code: "STALE_WRITE_PLAN" }),
      });
      return;
    }
    const token = generation;
    const executionOwner: Owner = {
      key,
      repositoryId:
        preview.type === "write" ? preview.value.repositoryId : null,
      verified: true,
    };
    update({ activity: "executing", error: null });
    let baseline: OperationRecord | null;
    try {
      baseline = await api.readOperation(null);
    } catch (error: unknown) {
      if (current(token))
        update({ activity: "idle", error: normalizeOperationError(error) });
      return;
    }
    if (!current(token)) {
      if (active) update({ activity: "idle", preview: null });
      return;
    }
    if (baseline && !baseline.result) {
      update({
        activity: "idle",
        error: normalizeOperationError({
          code: "OPERATION_IN_PROGRESS",
          retryable: true,
        }),
      });
      return;
    }
    recovery = {
      baseline: baseline?.progress.handle.operationId ?? null,
      kind: preview.type === "write" ? preview.value.kind : "clone",
      owner: executionOwner,
    };
    try {
      const handle =
        preview.type === "write"
          ? await api.executeWrite(
              preview.value.repositoryId,
              preview.value.planId,
            )
          : await api.executeClone(preview.value.planId);
      if (!active) return;
      owner = executionOwner;
      recovery = null;
      update({ handle, record: null, preview: null, activity: "running" });
      await poll();
    } catch (_error: unknown) {
      if (active && enabled) {
        update({ preview: null });
        await recoverExecution();
      }
    }
  }
  /** 启动或恢复时只查询；不把会话历史完成项自动应用到当前项目。 */
  async function resume(): Promise<void> {
    if (
      !active ||
      !enabled ||
      queryPending ||
      state.activity === "preparing" ||
      state.preview !== null
    )
      return;
    if (recovery) {
      await recoverExecution();
      return;
    }
    if (state.handle && !state.record?.result) {
      update({ activity: "running", error: null });
      await poll();
      return;
    }
    const token = ++queryGeneration;
    queryPending = true;
    update({ activity: "querying", error: null });
    try {
      const record = await api.readOperation(null);
      if (!active || !enabled || token !== queryGeneration) return;
      if (!record) {
        update({ activity: "idle" });
        return;
      }
      owner = {
        key,
        repositoryId: record.progress.handle.repositoryId,
        verified: false,
      };
      update({ handle: record.progress.handle, record: null });
      await accept(record, false);
      if (!record.result)
        cancelTimer = later(() => {
          void poll();
        }, 500);
    } catch (error: unknown) {
      if (active && enabled && token === queryGeneration)
        update({
          activity: "unverified",
          error: normalizeOperationError(error),
        });
    } finally {
      if (token === queryGeneration) queryPending = false;
    }
  }
  /** 卸载时丢弃准备与查询结果，后台执行继续存在。 */
  function deactivate(): void {
    active = false;
    generation += 1;
    stopQuery();
    update({
      preview: null,
      ...(state.activity === "preparing" ? { activity: "idle" as const } : {}),
    });
  }
  /** 严格模式重挂载恢复订阅；调用方随后显式查询当前任务。 */
  function activate(): void {
    active = true;
  }
  /** 为 React 订阅提供稳定的快照引用。 */
  function getSnapshot(): OperationsViewState {
    return state;
  }
  /** 订阅状态并返回卸载清理函数。 */
  function subscribe(listener: () => void): () => void {
    listeners.add(listener);
    return () => {
      listeners.delete(listener);
    };
  }
  return {
    setRepository,
    setCompletionHandler,
    prepareLocal,
    prepareRemote,
    prepareConflict,
    prepareClone,
    discardPreview,
    confirm,
    resume,
    activate,
    deactivate,
    getSnapshot,
    subscribe,
  };
}
