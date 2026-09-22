import { normalizeOperationError } from "../services/gitErrors.ts";
import type {
  ConflictDocument,
  ConflictState,
  ConflictWriteRequest,
  OperationError,
  RepositoryState,
} from "../types/git.ts";
export interface ConflictsApi {
  readConflicts(repositoryId: string): Promise<ConflictState>;
  readConflictDocument(
    repositoryId: string,
    sessionId: string,
    conflictId: string,
  ): Promise<ConflictDocument>;
}
export interface ConflictsSnapshot {
  conflicts: ConflictState | null;
  document: ConflictDocument | null;
  draft: string;
  dirty: boolean;
  loading: boolean;
  documentLoading: boolean;
  stale: boolean;
  needsDiscard: boolean;
  error: OperationError | null;
}
/** 将冲突草稿与原始指纹成对保留，读取失败不能悄悄覆盖编辑内容。 */
export function createConflictsController(api: ConflictsApi) {
  let state: ConflictsSnapshot = {
    conflicts: null,
    document: null,
    draft: "",
    dirty: false,
    loading: false,
    documentLoading: false,
    stale: false,
    needsDiscard: false,
    error: null,
  };
  let repository: RepositoryState | null = null;
  let active = true;
  let generation = 0;
  let fileGeneration = 0;
  const listeners = new Set<() => void>();
  /** 发布完整快照，卸载后显式状态变更不通知旧订阅者。 */
  function update(next: Partial<ConflictsSnapshot>): void {
    state = { ...state, ...next };
    if (active) for (const listener of listeners) listener();
  }
  /** 当前状态必须属于有真实 merge 标记的仓库。 */
  function allowed(): boolean {
    return active && repository?.operations.includes("merge") === true;
  }
  /** 绑定新上下文，未保存草稿保留为陈旧内容，不允许再次写回。 */
  function setRepository(next: RepositoryState | null): void {
    if (
      repository?.repositoryId === next?.repositoryId &&
      repository?.snapshotId === next?.snapshotId
    )
      return;
    repository = next;
    generation += 1;
    fileGeneration += 1;
    const common = { loading: false, documentLoading: false, error: null };
    update(
      state.dirty
        ? { ...common, stale: true, needsDiscard: true }
        : {
            ...common,
            conflicts: null,
            document: null,
            draft: "",
            dirty: false,
            stale: false,
            needsDiscard: false,
          },
    );
  }
  /** 刷新会作废旧文档能力；只有显式丢弃才能替换未保存草稿。 */
  async function refresh(discard = false): Promise<void> {
    if (!allowed() || !repository) return;
    if (state.dirty && !discard) {
      update({ needsDiscard: true });
      return;
    }
    if (state.loading) return;
    const repoId = repository.repositoryId;
    const token = ++generation;
    fileGeneration += 1;
    update({
      loading: true,
      documentLoading: false,
      stale: true,
      error: null,
      needsDiscard: false,
      ...(discard ? { document: null, draft: "", dirty: false } : {}),
    });
    try {
      const conflicts = await api.readConflicts(repoId);
      if (!allowed() || token !== generation) return;
      if (conflicts.repositoryId !== repoId)
        throw { code: "STALE_CONFLICT", retryable: true };
      update({
        conflicts,
        document: null,
        draft: "",
        dirty: false,
        loading: false,
        stale: false,
      });
    } catch (error: unknown) {
      if (active && token === generation)
        update({ loading: false, error: normalizeOperationError(error) });
    }
  }
  /** 只读取当前清单签发的受支持文件；文件请求单独排序。 */
  async function selectFile(
    conflictId: string,
    discard = false,
  ): Promise<void> {
    const conflicts = state.conflicts;
    if (
      !allowed() ||
      !repository ||
      state.loading ||
      !conflicts ||
      conflicts.repositoryId !== repository.repositoryId
    )
      return;
    if (
      !conflicts.files.some(
        (file) =>
          file.conflictId === conflictId &&
          file.editorSupport.status === "supported",
      )
    )
      return;
    if (state.dirty && !discard) {
      update({ needsDiscard: true });
      return;
    }
    const token = generation;
    const fileToken = ++fileGeneration;
    const repoId = repository.repositoryId;
    const session = conflicts.mergeSessionId;
    update({
      documentLoading: true,
      error: null,
      needsDiscard: false,
      ...(discard ? { document: null, draft: "", dirty: false } : {}),
    });
    try {
      const document = await api.readConflictDocument(
        repoId,
        session,
        conflictId,
      );
      if (!allowed() || token !== generation || fileToken !== fileGeneration)
        return;
      if (
        document.mergeSessionId !== session ||
        document.conflictId !== conflictId
      )
        throw { code: "STALE_CONFLICT", retryable: true };
      update({
        document,
        draft: document.result,
        documentLoading: false,
        dirty: false,
        stale: false,
      });
    } catch (error: unknown) {
      if (active && token === generation && fileToken === fileGeneration)
        update({
          documentLoading: false,
          error: normalizeOperationError(error),
        });
    }
  }
  /** 编辑不会解除 stale；读取期间禁用编辑，避免返回内容盖掉新输入。 */
  function edit(content: string): void {
    if (state.document && !state.loading && !state.documentLoading)
      update({
        draft: content,
        dirty: content !== state.document.result,
        error: null,
      });
  }
  /** 只确认刚提交的同一份草稿；后来的输入保留为未保存。 */
  function acknowledgeSave(conflictId: string, content?: string): void {
    if (
      state.document?.conflictId === conflictId &&
      (content === undefined || state.draft === content)
    )
      update({ dirty: false, stale: true });
  }
  /** 沿用展示文档的指纹，陈旧或正在读取的内容不能准备保存。 */
  function saveRequest(): Extract<
    ConflictWriteRequest,
    { kind: "saveConflict" }
  > | null {
    if (
      !allowed() ||
      !state.document ||
      state.loading ||
      state.documentLoading ||
      state.stale ||
      state.document.mergeSessionId !== state.conflicts?.mergeSessionId ||
      state.conflicts.repositoryId !== repository?.repositoryId
    )
      return null;
    return {
      kind: "saveConflict",
      conflictId: state.document.conflictId,
      fingerprint: state.document.fingerprint,
      content: state.draft,
    };
  }
  /** 严格模式重挂载重新启用订阅。 */
  function activate(): void {
    active = true;
  }
  /** 卸载作废响应，仍保留草稿供当前控制器重新挂载。 */
  function deactivate(): void {
    active = false;
    generation += 1;
    fileGeneration += 1;
    update({ loading: false, documentLoading: false });
  }
  /** 提供 React 使用的稳定快照。 */
  function getSnapshot(): ConflictsSnapshot {
    return state;
  }
  /** 注册订阅并返回清理函数。 */
  function subscribe(listener: () => void): () => void {
    listeners.add(listener);
    return () => {
      listeners.delete(listener);
    };
  }
  return {
    setRepository,
    refresh,
    selectFile,
    edit,
    acknowledgeSave,
    saveRequest,
    activate,
    deactivate,
    getSnapshot,
    subscribe,
  };
}
