import type {
  EditableFile,
  OperationError,
  ProjectFileList,
  RepositoryState,
} from "../types/git.ts";
import { normalizeOperationError } from "../services/gitErrors.ts";
import {
  acknowledgeEditorSave,
  closeEditorTab,
  editEditorTab,
  openEditorDocument,
  type EditorTab,
} from "../ui/editorTabs.ts";

export interface FileEditorApi {
  repository(): RepositoryState | null;
  files(): ProjectFileList | null;
  blocked(): boolean;
  onSavingChange?(active: boolean): void;
  read(
    repository: RepositoryState,
    fileId: string,
    signal?: AbortSignal,
  ): Promise<EditableFile>;
  save(
    repository: RepositoryState,
    document: EditableFile,
    content: string,
  ): Promise<EditableFile>;
}
export interface FileEditorState {
  repositoryId: string | null;
  tabs: readonly EditorTab[];
  activePath: string | null;
  loading: boolean;
  savingPath: string | null;
  pendingClose: string | null;
  error: OperationError | null;
  notice: string | null;
}

/** 管理项目文档草稿和保存边界，不依赖编辑器组件或窗口实现。 */
export function createFileEditorController(api: FileEditorApi) {
  let state: FileEditorState = {
    repositoryId: null,
    tabs: [],
    activePath: null,
    loading: false,
    savingPath: null,
    pendingClose: null,
    error: null,
    notice: null,
  };
  let epoch = 0;
  let readSequence = 0;
  let activeRead: {
    path: string;
    controller: AbortController;
    promise: Promise<EditableFile>;
  } | null = null;
  const listeners = new Set<() => void>();
  /** 发布稳定状态供 React 订阅。 */
  function update(patch: Partial<FileEditorState>): void {
    const savingBefore = Boolean(state.savingPath);
    state = { ...state, ...patch };
    if (savingBefore !== Boolean(state.savingPath))
      api.onSavingChange?.(Boolean(state.savingPath));
    for (const listener of listeners) listener();
  }
  /** 所有已打开文档参与未保存保护。 */
  function dirty(): boolean {
    return state.tabs.some((tab) => tab.draft !== tab.baseline);
  }
  /** 只有明确丢弃或没有草稿时才能更换文档归属，保存中禁止切换。 */
  function setRepository(id: string | null, discard = false): boolean {
    if (state.repositoryId === id) return true;
    if (state.savingPath || (dirty() && !discard)) return false;
    epoch++;
    activeRead?.controller.abort();
    activeRead = null;
    readSequence++;
    update({
      repositoryId: id,
      tabs: [],
      activePath: null,
      loading: false,
      pendingClose: null,
      error: null,
      notice: null,
    });
    return true;
  }
  /** 只接受当前项目和快照签发的路径映射，不向后端传入任意路径。 */
  function binding(path: string): {
    repository: RepositoryState;
    fileId: string;
  } {
    const repository = api.repository();
    const files = api.files();
    if (
      !repository ||
      repository.repositoryId !== state.repositoryId ||
      files?.repositoryId !== repository.repositoryId ||
      !files.snapshotId
    )
      throw { code: "STALE_REQUEST" };
    const file = files.files.find((item) => item.path === path);
    if (!file) throw { code: "FILE_UNAVAILABLE" };
    return { repository, fileId: file.fileId };
  }
  /** 异步结果不能跨仓库或跨快照安装。 */
  function verify(
    token: number,
    repository: RepositoryState,
    filesSnapshot?: string,
  ): void {
    const latest = api.repository();
    if (
      epoch !== token ||
      state.repositoryId !== repository.repositoryId ||
      latest?.repositoryId !== repository.repositoryId ||
      !latest ||
      (filesSnapshot !== undefined && api.files()?.snapshotId !== filesSnapshot)
    )
      throw { code: "STALE_REQUEST" };
  }
  /** 打开已存在标签只切换焦点，新文件按需读取完整文档。 */
  async function open(path: string): Promise<void> {
    const existing = activeRead;
    if (existing && existing.path !== path) {
      existing.controller.abort();
      activeRead = null;
      readSequence++;
    }
    if (state.tabs.some((tab) => tab.document.path === path)) {
      update({ activePath: path, loading: false });
      return;
    }
    if (existing?.path === path)
      return existing.promise.then(
        () => undefined,
        () => undefined,
      );
    existing?.controller.abort();
    const sequence = ++readSequence;
    const token = epoch;
    let requestPromise: Promise<EditableFile> | null = null;
    update({ loading: true, error: null, notice: null });
    try {
      const { repository, fileId } = binding(path);
      const filesSnapshot = api.files()?.snapshotId;
      const controller = new AbortController();
      requestPromise = api.read(repository, fileId, controller.signal);
      activeRead = { path, controller, promise: requestPromise };
      const document = await requestPromise;
      if (sequence !== readSequence) return;
      verify(token, repository, filesSnapshot);
      if (document.path !== path || document.fileId !== fileId)
        throw { code: "STALE_REQUEST" };
      update({
        tabs: openEditorDocument(state.tabs, document),
        activePath: path,
      });
    } catch (error: unknown) {
      if (token === epoch && sequence === readSequence)
        update({ error: normalizeOperationError(error) });
    } finally {
      if (requestPromise && activeRead?.promise === requestPromise)
        activeRead = null;
      if (token === epoch && sequence === readSequence)
        update({ loading: false });
    }
  }
  /** 保存过程中仍可输入，成功结果只确认提交时捕获的那份文本。 */
  function edit(path: string, content: string): void {
    update({ tabs: editEditorTab(state.tabs, path, content) });
  }
  /** 明确放弃后重新读取，只有成功且草稿未继续变化时才替换原标签。 */
  async function reload(path: string, discard = false): Promise<boolean> {
    const tab = state.tabs.find((item) => item.document.path === path);
    if (
      !tab ||
      state.savingPath ||
      api.blocked() ||
      (tab.draft !== tab.baseline && !discard)
    )
      return false;
    const token = epoch;
    activeRead?.controller.abort();
    activeRead = null;
    const sequence = ++readSequence;
    let requestPromise: Promise<EditableFile> | null = null;
    update({ loading: true, error: null, notice: null });
    try {
      const { repository, fileId } = binding(path);
      const filesSnapshot = api.files()?.snapshotId;
      const controller = new AbortController();
      requestPromise = api.read(repository, fileId, controller.signal);
      activeRead = { path, controller, promise: requestPromise };
      const document = await requestPromise;
      if (sequence !== readSequence) return false;
      verify(token, repository, filesSnapshot);
      if (document.path !== path || document.fileId !== fileId)
        throw { code: "STALE_REQUEST" };
      if (state.tabs.find((item) => item.document.path === path) !== tab)
        throw { code: "FILE_CHANGED" };
      const replacement = openEditorDocument([], document);
      update({
        tabs: state.tabs.flatMap((item) =>
          item === tab ? replacement : [item],
        ),
        pendingClose: state.pendingClose === path ? null : state.pendingClose,
        notice: "已重新读取文件。",
      });
      return true;
    } catch (error: unknown) {
      if (token === epoch && sequence === readSequence)
        update({ error: normalizeOperationError(error) });
      return false;
    } finally {
      if (requestPromise && activeRead?.promise === requestPromise)
        activeRead = null;
      if (token === epoch && sequence === readSequence)
        update({ loading: false });
    }
  }
  /** 使用当前标签版本保存，返回后端落盘复读结果并保留保存期间的新输入。 */
  async function save(path: string): Promise<boolean> {
    const tab = state.tabs.find((item) => item.document.path === path);
    if (!tab || state.savingPath || api.blocked()) return false;
    if (tab.draft === tab.baseline) return true;
    const submitted = tab.draft;
    const token = epoch;
    // 保存已经使读取结果过时，同时取消其排队，避免无效重读占用文件屏障。
    activeRead?.controller.abort();
    activeRead = null;
    ++readSequence;
    update({ savingPath: path, loading: false, error: null, notice: null });
    try {
      const repository = api.repository();
      if (!repository || repository.repositoryId !== state.repositoryId)
        throw { code: "STALE_REQUEST" };
      // 已打开文档的保存能力独立于目录列表；始终保留原文件版本。
      const result = await api.save(repository, tab.document, submitted);
      if (
        token !== epoch ||
        api.repository()?.repositoryId !== state.repositoryId
      )
        return false;
      if (
        result.path !== path ||
        result.fileId !== tab.document.fileId ||
        result.text.content !== submitted
      )
        throw { code: "STALE_REQUEST" };
      update({
        tabs: acknowledgeEditorSave(state.tabs, path, result, submitted),
        notice: "文件已保存。",
      });
      return true;
    } catch (error: unknown) {
      if (token === epoch) update({ error: normalizeOperationError(error) });
      return false;
    } finally {
      if (token === epoch) update({ savingPath: null });
    }
  }
  /** 关闭仅影响目标标签；未明确放弃时脏文档进入确认状态。 */
  function close(path: string, discard = false): void {
    if (state.savingPath === path) return;
    const tabs = closeEditorTab(state.tabs, path, discard);
    if (tabs === state.tabs) {
      if (state.tabs.some((tab) => tab.document.path === path))
        update({ pendingClose: path });
      return;
    }
    readSequence++;
    if (activeRead?.path === path) {
      activeRead.controller.abort();
      activeRead = null;
    }
    update({
      tabs,
      activePath:
        state.activePath === path
          ? (tabs.at(-1)?.document.path ?? null)
          : state.activePath,
      loading: false,
      pendingClose: null,
    });
  }
  /** 取消关闭保留全部草稿。 */
  function cancelClose(): void {
    update({ pendingClose: null });
  }
  /** 外部文件版本变化时提示脏草稿，避免静默覆盖用户输入。 */
  function markExternalChange(): void {
    if (dirty()) update({ notice: "文件在外部发生变化，当前草稿仍保留。" });
  }
  /** 保存并关闭须确认保存期间没有新的输入，失败保持确认与草稿。 */
  async function saveAndClose(): Promise<void> {
    const path = state.pendingClose;
    if (path && (await save(path))) close(path);
  }
  /** 返回稳定快照。 */
  function getSnapshot(): FileEditorState {
    return state;
  }
  /** 注册状态订阅并释放。 */
  function subscribe(listener: () => void): () => void {
    listeners.add(listener);
    return () => {
      listeners.delete(listener);
    };
  }
  return {
    getSnapshot,
    subscribe,
    setRepository,
    dirty,
    open,
    reload,
    edit,
    save,
    close,
    cancelClose,
    markExternalChange,
    saveAndClose,
  };
}
