import { normalizeOperationError } from "../services/gitErrors.ts";
import type {
  OperationError,
  OperationResult,
  RemoteAssessment,
  RemoteState,
  RemoteWriteRequest,
  RepositoryState,
  SyncTarget,
} from "../types/git.ts";

export interface RemoteSyncApi {
  repository(): RepositoryState | null;
  blocked(): boolean;
  readRemotes(id: string): Promise<RemoteState>;
  readSyncTarget(id: string, snapshot: string): Promise<SyncTarget>;
  assessRemote(
    id: string,
    snapshot: string,
    branch: string,
  ): Promise<RemoteAssessment>;
  execute(
    repository: RepositoryState,
    request: RemoteWriteRequest,
  ): Promise<OperationResult>;
}
export interface RemoteSyncState {
  busy: boolean;
  phase: "idle" | "checking" | "fetching" | "assessing" | "syncing";
  notice: string | null;
  error: OperationError | null;
  diverged: boolean;
  setup: { target: SyncTarget; remotes: RemoteState } | null;
}

/** 串行同步当前分支；每一步重新读取真实状态，失败或上下文变化即停止后续写入。 */
export function createRemoteSyncController(api: RemoteSyncApi) {
  let state: RemoteSyncState = {
    busy: false,
    phase: "idle",
    notice: null,
    error: null,
    diverged: false,
    setup: null,
  };
  let generation = 0;
  let active = true;
  let setupSource: RepositoryState | null = null;
  const listeners = new Set<() => void>();
  /** 向 React 发布完整不可变状态。 */
  function update(patch: Partial<RemoteSyncState>): void {
    state = { ...state, ...patch };
    for (const listener of listeners) listener();
  }
  /** 在所有异步边界检查原始分支和提交，外部写入不能改变本次同步来源。 */
  function current(source: RepositoryState, token: number): RepositoryState {
    const next = api.repository();
    if (
      !active ||
      generation !== token ||
      !next ||
      next.repositoryId !== source.repositoryId ||
      next.head.kind !== "branch" ||
      source.head.kind !== "branch" ||
      next.head.name !== source.head.name ||
      next.head.oid !== source.head.oid
    )
      throw { code: "STALE_REQUEST" };
    if (api.blocked()) throw { code: "OPERATION_IN_PROGRESS" };
    if (next.changes.length || next.operations.length)
      throw { code: "WORKTREE_DIRTY" };
    return next;
  }
  /** 后端失败、未核实或冲突结果不能继续自动链。 */
  function succeeded(result: OperationResult): void {
    if (result.outcome === "failed" || result.outcome === "unknown")
      throw result.error;
    if (result.outcome !== "succeeded") throw { code: "CONFLICT_PRESENT" };
    if (result.refresh.status === "failed") throw result.refresh.error;
  }
  /** 用户点击同步后才允许联网；双击在同步门禁处合并。 */
  async function run(): Promise<void> {
    if (!active || state.busy || api.blocked()) return;
    const source = api.repository();
    if (!source) return;
    const token = ++generation;
    let pushed = false;
    update({
      busy: true,
      phase: "checking",
      notice: null,
      error: null,
      diverged: false,
      setup: null,
    });
    try {
      current(source, token);
      const remotes = await api.readRemotes(source.repositoryId);
      if (remotes.repositoryId !== source.repositoryId)
        throw { code: "STALE_REQUEST" };
      const initial = current(source, token);
      const target = await api.readSyncTarget(
        initial.repositoryId,
        initial.snapshotId,
      );
      current(source, token);
      if (
        target.repositoryId !== initial.repositoryId ||
        target.snapshotId !== initial.snapshotId
      )
        throw { code: "STALE_REQUEST" };
      if (target.upstream.status !== "configured") {
        setupSource = source;
        update({ setup: { target, remotes } });
        return;
      }
      const initialUpstream = target.upstream;
      const remoteName = remotes.remotes.find(
        (remote) => remote.remoteId === initialUpstream.remoteId,
      )?.name;
      if (!remoteName) throw { code: "STALE_REQUEST" };
      const targetBranch = target.upstream.targetBranchName;
      update({ phase: "fetching" });
      succeeded(
        await api.execute(current(source, token), {
          kind: "fetchAll",
          remoteId: target.upstream.remoteId,
        }),
      );
      current(source, token);
      update({ phase: "assessing" });
      const refreshed = await api.readRemotes(source.repositoryId);
      if (refreshed.repositoryId !== source.repositoryId)
        throw { code: "STALE_REQUEST" };
      const fresh = current(source, token);
      const next = await api.readSyncTarget(
        fresh.repositoryId,
        fresh.snapshotId,
      );
      current(source, token);
      if (
        next.repositoryId !== fresh.repositoryId ||
        next.snapshotId !== fresh.snapshotId ||
        next.upstream.status !== "configured"
      )
        throw { code: "REMOTE_CHANGED" };
      const upstream = next.upstream;
      if (
        upstream.targetBranchName !== targetBranch ||
        refreshed.remotes.find(
          (remote) => remote.remoteId === upstream.remoteId,
        )?.name !== remoteName
      )
        throw { code: "REMOTE_CHANGED" };
      if (!upstream.remoteBranchId) {
        setupSource = source;
        update({
          setup: { target: next, remotes: refreshed },
          notice: "远端目标不存在，请重新设置同步目标。",
        });
        return;
      }
      const assessment = await api.assessRemote(
        fresh.repositoryId,
        fresh.snapshotId,
        upstream.remoteBranchId,
      );
      const latest = current(source, token);
      if (
        latest.snapshotId !== fresh.snapshotId ||
        assessment.repositoryId !== fresh.repositoryId ||
        assessment.snapshotId !== fresh.snapshotId ||
        assessment.remoteBranchId !== upstream.remoteBranchId ||
        source.head.kind !== "branch" ||
        assessment.localOid !== source.head.oid
      )
        throw { code: "STALE_REQUEST" };
      if (assessment.relation === "equal") {
        update({ notice: "当前分支已与远端同步。" });
        return;
      }
      if (assessment.relation === "diverged") {
        update({
          diverged: true,
          notice: "本地与远端已分叉，请通过合并流程处理。",
        });
        return;
      }
      if (assessment.relation === "unrelated")
        throw { code: "NO_COMMON_ANCESTOR" };
      if (assessment.relation === "unknown")
        throw { code: "UNSUPPORTED_WRITE_CONFIGURATION" };
      update({ phase: "syncing" });
      const synchronization = await api.execute(latest, {
        kind: assessment.relation === "behind" ? "syncFastForward" : "syncPush",
      });
      pushed =
        assessment.relation === "ahead" &&
        synchronization.outcome === "succeeded";
      succeeded(synchronization);
      if (pushed) {
        // 私有推送不改本地跟踪引用；重新获取真实状态，不预测远端标签位置。
        current(source, token);
        update({ phase: "fetching", notice: "已推送，正在更新远端状态。" });
        const afterPush = await api.readRemotes(source.repositoryId);
        const selected = afterPush.remotes.find(
          (remote) => remote.name === remoteName,
        );
        if (!selected || afterPush.repositoryId !== source.repositoryId)
          throw { code: "REMOTE_CHANGED" };
        const pushedState = current(source, token);
        const pushedTarget = await api.readSyncTarget(
          pushedState.repositoryId,
          pushedState.snapshotId,
        );
        const fetchState = current(source, token);
        if (
          fetchState.snapshotId !== pushedState.snapshotId ||
          pushedTarget.repositoryId !== pushedState.repositoryId ||
          pushedTarget.snapshotId !== pushedState.snapshotId ||
          pushedTarget.upstream.status !== "configured" ||
          pushedTarget.upstream.remoteId !== selected.remoteId ||
          pushedTarget.upstream.targetBranchName !== targetBranch
        )
          throw { code: "REMOTE_CHANGED" };
        succeeded(
          await api.execute(fetchState, {
            kind: "fetchAll",
            remoteId: selected.remoteId,
          }),
        );
        current(source, token);
      }
      // 快进会改变 HEAD，结束只核对会话；后端终态已经核验实际目标与本地状态。
      if (
        !active ||
        generation !== token ||
        api.repository()?.repositoryId !== source.repositoryId
      )
        return;
      const final = api.repository();
      if (
        final?.head.kind !== "branch" ||
        source.head.kind !== "branch" ||
        final.head.name !== source.head.name
      )
        throw { code: "STALE_REQUEST" };
      update({ notice: "当前分支同步完成。" });
    } catch (error: unknown) {
      if (active && token === generation)
        update({
          error: normalizeOperationError(error),
          ...(pushed
            ? { notice: "已推送，但后续状态核验未完成，请刷新后查看。" }
            : {}),
        });
    } finally {
      if (active && token === generation)
        update({ busy: false, phase: "idle" });
    }
  }
  /** 用户明确选择目标后先获取核验；新目标需显式允许创建，成功后才设置上游。 */
  async function configure(
    remoteId: string,
    branchName: string,
    allowCreate: boolean,
  ): Promise<void> {
    if (!active || state.busy || api.blocked() || !state.setup || !setupSource)
      return;
    const remoteName = state.setup.remotes.remotes.find(
      (remote) => remote.remoteId === remoteId,
    )?.name;
    const targetBranchName = branchName.trim();
    if (!remoteName || !targetBranchName) return;
    const source = setupSource;
    const token = ++generation;
    let published = false;
    let upstreamWritten = false;
    let configured = false;
    update({ busy: true, phase: "checking", error: null, notice: null });
    try {
      current(source, token);
      let remotes = await api.readRemotes(source.repositoryId);
      let selected = remotes.remotes.find(
        (remote) => remote.name === remoteName,
      );
      if (!selected || remotes.repositoryId !== source.repositoryId)
        throw { code: "REMOTE_CHANGED" };
      update({ phase: "fetching" });
      succeeded(
        await api.execute(current(source, token), {
          kind: "fetchAll",
          remoteId: selected.remoteId,
        }),
      );
      current(source, token);
      remotes = await api.readRemotes(source.repositoryId);
      selected = remotes.remotes.find((remote) => remote.name === remoteName);
      if (
        !selected ||
        remotes.repositoryId !== source.repositoryId ||
        selected.fetchedBranchNames === null
      )
        throw { code: "REMOTE_CHANGED" };
      if (!selected.fetchedBranchNames.includes(targetBranchName)) {
        if (!allowCreate) throw { code: "SYNC_TARGET_MISSING" };
        update({ phase: "syncing" });
        const publication = await api.execute(current(source, token), {
          kind: "publishBranch",
          remoteId: selected.remoteId,
          targetBranchName,
        });
        // 写入成功与后续刷新分开记录，刷新失败不能抹去已发生的发布。
        published = publication.outcome === "succeeded";
        succeeded(publication);
        current(source, token);
        update({ notice: "远端分支已发布，正在设置上游。", phase: "fetching" });
        // 推送不修改本地跟踪引用，必须重新获取已验证的远端目标再设置。
        remotes = await api.readRemotes(source.repositoryId);
        selected = remotes.remotes.find((remote) => remote.name === remoteName);
        if (!selected || remotes.repositoryId !== source.repositoryId)
          throw { code: "REMOTE_CHANGED" };
        succeeded(
          await api.execute(current(source, token), {
            kind: "fetchAll",
            remoteId: selected.remoteId,
          }),
        );
        current(source, token);
        remotes = await api.readRemotes(source.repositoryId);
        selected = remotes.remotes.find((remote) => remote.name === remoteName);
        if (
          !selected?.fetchedBranchNames?.includes(targetBranchName) ||
          remotes.repositoryId !== source.repositoryId
        )
          throw { code: "REMOTE_CHANGED" };
      }
      update({ phase: "syncing" });
      const configuration = await api.execute(current(source, token), {
        kind: "setUpstream",
        remoteId: selected.remoteId,
        targetBranchName,
      });
      upstreamWritten = configuration.outcome === "succeeded";
      succeeded(configuration);
      current(source, token);
      configured = true;
      update({ setup: null, notice: "上游已设置，继续同步当前分支。" });
    } catch (error: unknown) {
      if (active && token === generation)
        update({
          error: normalizeOperationError(error),
          ...(upstreamWritten
            ? { notice: "上游已设置，但后续状态核验未完成，请刷新后重试同步。" }
            : published
              ? {
                  notice:
                    "远端分支已发布，但上游设置尚未完成。再次尝试会先核对远端。",
                }
              : {}),
        });
    } finally {
      if (active && token === generation)
        update({ busy: false, phase: "idle" });
    }
    if (configured && active && generation === token) {
      await run();
      // run 会清除上一阶段提示，仅在同一自动链失败时补回已完成事实。
      if (active && generation === token + 1 && state.error)
        update({
          notice: "上游已设置，但后续同步未完成，请处理错误后重试同步。",
        });
    }
  }
  /** 取消目标设置不写配置，也不取消已经在后台运行的任务。 */
  function closeSetup(): void {
    if (!state.busy) {
      setupSource = null;
      update({ setup: null });
    }
  }
  /** 仓库切换或卸载只中断后续编排，不取消已经执行的后台任务。 */
  function reset(): void {
    ++generation;
    setupSource = null;
    update({
      busy: false,
      phase: "idle",
      notice: null,
      error: null,
      diverged: false,
      setup: null,
    });
  }
  /** 严格模式重挂载恢复交互。 */
  function activate(): void {
    active = true;
  }
  /** 停止后续界面写入。 */
  function deactivate(): void {
    active = false;
    reset();
  }
  /** 返回稳定快照供外部订阅。 */
  function getSnapshot(): RemoteSyncState {
    return state;
  }
  /** 注册一个视图订阅并返回清理方法。 */
  function subscribe(listener: () => void): () => void {
    listeners.add(listener);
    return () => {
      listeners.delete(listener);
    };
  }
  return {
    run,
    configure,
    closeSetup,
    reset,
    activate,
    deactivate,
    getSnapshot,
    subscribe,
  };
}
