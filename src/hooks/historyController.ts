import type {
  CommitDetail,
  CommitFileList,
  FileDiff,
  HistoryPage,
  OperationError,
  RepositoryState,
} from "../types/git.ts";
import { normalizeOperationError } from "../services/gitErrors.ts";

export interface HistoryApi {
  readCommitHistory: (
    repositoryId: string,
    cursor: string | null,
  ) => Promise<HistoryPage>;
  readCommitDetail: (
    repositoryId: string,
    graphSnapshotId: string,
    oid: string,
  ) => Promise<CommitDetail>;
  readCommitFiles: (
    repositoryId: string,
    graphSnapshotId: string,
    oid: string,
    parentOid: string | null,
  ) => Promise<CommitFileList>;
  readCommitFileDiff: (
    repositoryId: string,
    graphSnapshotId: string,
    fileId: string,
  ) => Promise<FileDiff>;
}
export interface HistoryState {
  page: HistoryPage | null;
  loading: boolean;
  loadingMore: boolean;
  selectedOid: string | null;
  detail: CommitDetail | null;
  files: CommitFileList | null;
  diff: FileDiff | null;
  detailLoading: boolean;
  filesLoading: boolean;
  diffLoading: boolean;
  error: OperationError | null;
}
/** 创建空历史状态，保留字段完整性。 */
const emptyState = (): HistoryState => ({
  page: null,
  loading: false,
  loadingMore: false,
  selectedOid: null,
  detail: null,
  files: null,
  diff: null,
  detailLoading: false,
  filesLoading: false,
  diffLoading: false,
  error: null,
});
/** 构造与 React/IPC 解耦的历史读取控制器。 */
export function createHistoryController(api: HistoryApi) {
  let state = emptyState();
  let repository: RepositoryState | null = null;
  let active = true;
  let generation = 0;
  let selectionGeneration = 0;
  let filesGeneration = 0;
  let diffGeneration = 0;
  const listeners = new Set<() => void>();
  const historyRequests = new Set<string>();
  /** 发布不可变状态快照。 */
  const update = (next: Partial<HistoryState>): void => {
    state = { ...state, ...next };
    listeners.forEach((listener) => listener());
  };
  /** 清理选择并使其所有旧请求失效。 */
  const clearSelection = (): void => {
    selectionGeneration += 1;
    filesGeneration += 1;
    diffGeneration += 1;
    update({
      selectedOid: null,
      detail: null,
      files: null,
      diff: null,
      detailLoading: false,
      filesLoading: false,
      diffLoading: false,
    });
  };
  /** 读取历史页并校验仓库/图代次。 */
  async function load(cursor: string | null): Promise<void> {
    if (!active || !repository) return;
    const owner = generation;
    const id = repository.repositoryId;
    const key = `${owner}:${id}:${cursor ?? "root"}`;
    if (historyRequests.has(key)) return;
    historyRequests.add(key);
    update(
      cursor === null
        ? { loading: true, error: null }
        : { loadingMore: true, error: null },
    );
    try {
      const page = await api.readCommitHistory(id, cursor);
      if (!active || owner !== generation) return;
      if (page.repositoryId !== id) throw { code: "STALE_GRAPH" };
      if (cursor === null) update({ page, loading: false });
      else if (state.page?.graphSnapshotId === page.graphSnapshotId)
        update({
          page: {
            ...state.page,
            commits: [
              ...state.page.commits,
              ...page.commits.filter(
                (commit) =>
                  !state.page?.commits.some((old) => old.oid === commit.oid),
              ),
            ],
            nextCursor: page.nextCursor,
          },
          loadingMore: false,
        });
      else
        update({
          loadingMore: false,
          error: normalizeOperationError({
            code: "STALE_GRAPH",
            retryable: false,
          }),
        });
    } catch (error: unknown) {
      if (active && owner === generation)
        update(
          cursor === null
            ? { loading: false, error: normalizeOperationError(error) }
            : { loadingMore: false, error: normalizeOperationError(error) },
        );
    } finally {
      if (owner === generation) historyRequests.delete(key);
    }
  }
  /** 选择已加载提交，详情成功后读取默认父文件列表。 */
  async function selectCommit(oid: string): Promise<void> {
    if (
      !active ||
      !repository ||
      !state.page?.commits.some((commit) => commit.oid === oid)
    )
      return;
    const owner = generation;
    const selection = ++selectionGeneration;
    filesGeneration += 1;
    diffGeneration += 1;
    const graph = state.page.graphSnapshotId;
    update({
      selectedOid: oid,
      detail: null,
      files: null,
      diff: null,
      detailLoading: true,
      filesLoading: false,
      diffLoading: false,
      error: null,
    });
    try {
      const detail = await api.readCommitDetail(
        repository.repositoryId,
        graph,
        oid,
      );
      if (active && owner === generation && selection === selectionGeneration) {
        if (detail.oid !== oid) throw { code: "STALE_GRAPH" };
        update({ detail, detailLoading: false });
        void selectParent(null);
      }
    } catch (error: unknown) {
      if (active && owner === generation && selection === selectionGeneration)
        update({ detailLoading: false, error: normalizeOperationError(error) });
    }
  }
  /** 读取指定父提交或根提交默认差异。 */
  async function selectParent(parentOid: string | null): Promise<void> {
    if (
      !active ||
      !repository ||
      !state.detail ||
      (parentOid !== null && !state.detail.parentOids.includes(parentOid))
    )
      return;
    const owner = generation;
    const selection = selectionGeneration;
    const request = ++filesGeneration;
    diffGeneration += 1;
    const detail = state.detail;
    const graph = state.page?.graphSnapshotId ?? "";
    update({
      files: null,
      diff: null,
      filesLoading: true,
      diffLoading: false,
      error: null,
    });
    try {
      const files = await api.readCommitFiles(
        repository.repositoryId,
        graph,
        detail.oid,
        parentOid,
      );
      if (
        active &&
        owner === generation &&
        selection === selectionGeneration &&
        request === filesGeneration
      ) {
        const expectedParent = parentOid ?? detail.parentOids[0] ?? null;
        if (
          files.repositoryId !== repository.repositoryId ||
          files.graphSnapshotId !== graph ||
          files.oid !== detail.oid ||
          files.parentOid !== expectedParent
        )
          throw { code: "STALE_GRAPH" };
        update({ files, filesLoading: false });
      }
    } catch (error: unknown) {
      if (
        active &&
        owner === generation &&
        selection === selectionGeneration &&
        request === filesGeneration
      )
        update({ filesLoading: false, error: normalizeOperationError(error) });
    }
  }
  /** 读取文件差异并阻止旧选择覆盖新选择。 */
  async function selectFile(fileId: string): Promise<void> {
    if (
      !active ||
      !repository ||
      !state.files ||
      !state.detail ||
      !state.files.files.some((file) => file.fileId === fileId)
    )
      return;
    const owner = generation;
    const selection = selectionGeneration;
    const request = ++diffGeneration;
    const graph = state.files.graphSnapshotId;
    const oid = state.files.oid;
    update({ diff: null, diffLoading: true, error: null });
    try {
      const diff = await api.readCommitFileDiff(
        repository.repositoryId,
        graph,
        fileId,
      );
      if (
        active &&
        owner === generation &&
        selection === selectionGeneration &&
        request === diffGeneration &&
        state.files?.repositoryId === repository.repositoryId &&
        state.files.graphSnapshotId === graph &&
        state.files.oid === oid
      )
        update({ diff, diffLoading: false });
    } catch (error: unknown) {
      if (
        active &&
        owner === generation &&
        selection === selectionGeneration &&
        request === diffGeneration
      )
        update({ diffLoading: false, error: normalizeOperationError(error) });
    }
  }
  return {
    /** 返回当前不可变快照。 */
    getSnapshot: (): HistoryState => state,
    /** 订阅并提供清理函数。 */
    subscribe: (listener: () => void): (() => void) => {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    /** 恢复挂载生命周期。 */
    activate: (): void => {
      active = true;
    },
    /** 卸载时作废所有读取。 */
    deactivate: (): void => {
      active = false;
      generation += 1;
      selectionGeneration += 1;
      filesGeneration += 1;
      diffGeneration += 1;
    },
    /** 新快照重建历史会话。 */
    setRepository: (next: RepositoryState | null): void => {
      if (
        repository?.repositoryId === next?.repositoryId &&
        repository?.snapshotId === next?.snapshotId
      )
        return;
      repository = next;
      generation += 1;
      historyRequests.clear();
      clearSelection();
      update({ page: null, loading: false, loadingMore: false, error: null });
    },
    /** 显式刷新开始一个新图。 */
    refresh: (): Promise<void> => {
      generation += 1;
      historyRequests.clear();
      clearSelection();
      update({ page: null, loading: false, loadingMore: false, error: null });
      return load(null);
    },
    /** 仅加载当前快照的后续游标。 */
    loadMore: (): Promise<void> =>
      state.page?.nextCursor ? load(state.page.nextCursor) : Promise.resolve(),
    selectCommit,
    selectParent,
    selectFile,
  };
}
