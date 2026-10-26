import { useEditorAutosave } from "./useEditorAutosave";
import { useWorkbenchShortcuts } from "./useWorkbenchShortcuts";
import { useChangeDetailFocus } from "./useChangeDetailFocus";
import { useLogSettings } from "./useLogSettings";
import { useManualRefresh } from "./useManualRefresh";
import { useRemoteSync } from "./useRemoteSync";
import { useFileEditor } from "./useFileEditor";
import { readBranchesWithRetry } from "./branchRead";
import { useRecentProjects } from "./useRecentProjects";
import { useWorkspaceSession } from "./useWorkspaceSession";
import { restoreTreePaths } from "../ui/restoreTreePaths";
import type { WorkspaceSession } from "../types/workspaceSession";
import {
  toggleGroupSelection,
  reconcileChangeSelection,
} from "../ui/virtualChanges";
import { mergeTreeFiles } from "../ui/projectTree";
import { selectionAction } from "../ui/selectionAction";
import { useEffect, useMemo, useRef, useState, type ChangeEvent } from "react";
import { useWorkspace } from "./useWorkspace";
import { useAppSettings } from "./useAppSettings";
import type { SettingsCategory } from "../types/settings";
import * as api from "../services/git";
import { normalizeOperationError } from "../services/gitErrors";
import { describeGitError, groupChanges } from "../ui/gitPresentation";
import type {
  BranchList,
  CloneParent,
  DiffSide,
  FileDiff,
  OperationError,
  OperationKind,
  ProjectFileList,
  ProjectTreePage,
  WriteContext,
} from "../types/git";

export type Drawer = "changes" | "files" | "detail" | "conflicts" | null;
export type Modal = "clone" | "branch" | "remote" | "settings" | null;
type Field =
  | "message"
  | "branchName"
  | "cloneUrl"
  | "directoryName"
  | "targetBranch"
  | "mergeMessage";
/** 汇总工作台的表单、资源读取与导航，组件只负责呈现。 */
export function useWorkbench() {
  const workspace = useWorkspace();
  const { repo, operations, conflicts } = workspace;
  const repository = repo.repository;
  // 选择和草稿更新不重新过滤整个仓库的变更清单。
  const changeGroups = useMemo(
    () => groupChanges(repository?.changes ?? []),
    [repository?.changes],
  );
  const editorGate = useRef<ReturnType<typeof useFileEditor> | null>(null);
  const documentsVersion = repository?.versions?.documents;
  const previousDocumentsVersion = useRef<number | undefined>(documentsVersion);
  const manualRefresh = useManualRefresh(
    repo,
    workspace.preview || operations.busy || repo.loading || conflicts.dirty,
    operations.runRemote,
    repo.refresh,
    repo.getSnapshot,
    operations.getSnapshot,
  );
  const logSettings = useLogSettings(!workspace.preview);
  const syncRemote = useRemoteSync(
    {
      repository: () => {
        const current = repo.getSnapshot();
        return current.loading || current.stale ? null : current.repository;
      },
      blocked: () =>
        workspace.preview ||
        conflicts.dirty ||
        Boolean(editorGate.current?.dirty || editorGate.current?.savingPath) ||
        manualRefresh.busy ||
        operations.getSnapshot().busy,
      readRemotes: api.readRemotes,
      readSyncTarget: api.readSyncTarget,
      assessRemote: api.assessRemote,
      execute: operations.runRemote,
    },
    repository?.repositoryId,
  );
  useEffect(() => {
    workspace.setRemoteWorkflowActive(syncRemote.busy || manualRefresh.busy);
    return () => workspace.setRemoteWorkflowActive(false);
  }, [workspace.setRemoteWorkflowActive, syncRemote.busy, manualRefresh.busy]);
  const appSettings = useAppSettings(
    !workspace.preview,
    settingsApplied,
    () =>
      !operations.getSnapshot().busy &&
      !conflicts.dirty &&
      !editorGate.current?.dirty &&
      workspace.git.status !== "loading",
  );
  const preferences = {
    saved: appSettings.saved?.settings.uiPreferences ?? null,
  };
  const [settingsPending, setSettingsPending] = useState<
    "close" | "reload" | "exit" | null
  >(null);
  const settingsExit = useRef<(() => Promise<void>) | null>(null);
  /** 持久化成功才更新运行环境；普通外观设置不会清空项目。 */
  function settingsApplied(gitChanged: boolean): void {
    logSettings.reload();
    if (gitChanged) {
      repo.clear();
      void workspace.git.refresh();
    }
  }
  /** 应用设置失败时保持表单与所有输入。 */
  async function applySettings(): Promise<void> {
    await appSettings.save();
  }
  /** 恢复当前分类仅变更内存草稿。 */
  function restoreSettings(): void {
    appSettings.restore(settingsCategory);
  }
  /** 重新读取前处理尚未保存的编辑。 */
  function reloadSettings(): void {
    if (appSettings.dirty) setSettingsPending("reload");
    else void appSettings.reload();
  }
  /** 取消待执行的关闭或重读，保留编辑。 */
  function keepSettings(): void {
    if (settingsPending === "exit") setModal("settings");
    settingsExit.current = null;
    setSettingsPending(null);
  }
  /** 用户明确丢弃后执行原动作。 */
  function discardSettings(): void {
    if (appSettings.activity !== "idle") return;
    const next = settingsPending;
    setSettingsPending(null);
    appSettings.cancel();
    if (next === "reload") void appSettings.reload();
    else if (next === "close") setModal(null);
    else if (next === "exit") {
      const action = settingsExit.current;
      settingsExit.current = null;
      setModal(null);
      void workspaceSession.flush().then(() => action?.());
    }
  }
  /** 保存并关闭只有在持久化成功后才离开。 */
  async function saveSettingsAndContinue(): Promise<void> {
    const next = settingsPending;
    if (!(await appSettings.save())) return;
    setSettingsPending(null);
    if (next === "reload") void appSettings.reload();
    else if (next === "close") setModal(null);
    else if (next === "exit") {
      const action = settingsExit.current;
      settingsExit.current = null;
      setModal(null);
      await workspaceSession.flush();
      await action?.();
    }
  }
  const [drawer, setDrawer] = useState<Drawer>(null);
  const [changeDetailOpen, setChangeDetailOpen] = useState(false);
  const changeDetailFocus = useChangeDetailFocus(
    drawer === "changes" && changeDetailOpen && repo.selected !== null,
    closeChangeDetail,
  );
  useEffect(() => {
    if (drawer !== "changes") {
      repo.closeDiff();
      setChangeDetailOpen(false);
    }
  }, [drawer, repo.closeDiff]);
  const [modal, setModal] = useState<Modal>(null);
  const [showOperations, setShowOperations] = useState(false);
  const [branchesExpanded, setBranchesExpanded] = useState(true);
  const [projectMenu, setProjectMenu] = useState(false);
  const recent = useRecentProjects();
  const [settingsCategory, setSettingsCategory] =
    useState<SettingsCategory>("general");
  const [branches, setBranches] = useState<BranchList | null>(null);
  const loadedBranchesKey = useRef<string | null>(null);
  const [context, setContext] = useState<WriteContext | null>(null);
  const [resourceErrors, setResourceErrors] = useState<
    Partial<
      Record<
        "branches" | "context" | "files" | "preview" | "chooser",
        OperationError | null
      >
    >
  >({});
  const resourceError = Object.values(resourceErrors).find(Boolean) ?? null;
  /** 资源独立记录失败，成功只清除自身错误；过期后台响应不显示为用户故障。 */
  function setResourceError(
    error: OperationError | null,
    source: keyof typeof resourceErrors = "chooser",
  ): void {
    if (error?.code === "STALE_REQUEST" || error?.code === "REQUEST_CANCELLED")
      return;
    setResourceErrors((previous) => ({ ...previous, [source]: error }));
  }
  const [projectFiles, setProjectFiles] = useState<ProjectFileList | null>(
    null,
  );
  const [projectTree, setProjectTree] = useState<ProjectTreePage | null>(null);
  const treeCurrent = useRef(projectTree);
  const loadedTreeKey = useRef<string | null>(null);
  treeCurrent.current = projectTree;
  const [projectDiff, setProjectDiff] = useState<FileDiff | null>(null);
  const [includeIgnored, setIncludeIgnored] = useState(false);
  const [projectPath, setProjectPath] = useState("");
  const [projectLoading, setProjectLoading] = useState(false);
  const fileList = useRef(projectFiles);
  const listRequest = useRef<{ key: string; promise: Promise<void> } | null>(
    null,
  );
  fileList.current = projectFiles;
  const editor = useFileEditor(
    {
      repository: () => repo.getSnapshot().repository,
      files: () => fileList.current,
      onSavingChange: workspace.setFileSaveActive,
      blocked: () =>
        workspace.preview ||
        operations.getSnapshot().busy ||
        syncRemote.busy ||
        (repo.getSnapshot().loading && repo.getSnapshot().openingPath !== null),
      read: (state, id, signal) =>
        api.readEditableFile(
          state.repositoryId,
          fileList.current?.snapshotId ?? state.snapshotId,
          id,
          signal,
        ),
      save: async (state, document, content) => {
        const result = await api.saveEditorFile(
          state.repositoryId,
          document.fileId,
          document.version,
          content,
        );
        if (repo.getSnapshot().repository?.repositoryId === state.repositoryId)
          repo.markDirty();
        return result;
      },
    },
    repository?.repositoryId ?? null,
  );
  useEffect(() => {
    if (
      documentsVersion === undefined ||
      previousDocumentsVersion.current === undefined ||
      documentsVersion === previousDocumentsVersion.current
    ) {
      previousDocumentsVersion.current = documentsVersion;
      return;
    }
    if (repo.loading || operations.busy || editor.savingPath || !projectFiles)
      return;
    // 拓扑刷新保留旧视图，但重读必须等待当前目录版本的新能力安装。
    if (
      loadedTreeKey.current !==
      JSON.stringify([
        repository?.repositoryId,
        repository?.versions?.files ?? repository?.snapshotId,
        includeIgnored,
      ])
    )
      return;
    editor.controller.markExternalChange();
    let cancelled = false;
    // 低优先级批次遇到用户打开、关闭或保存时停止，不能取消新的前台读取。
    void (async () => {
      if (await editor.controller.reloadCleanDocuments(() => !cancelled))
        previousDocumentsVersion.current = documentsVersion;
    })();
    return () => {
      cancelled = true;
    };
  }, [
    documentsVersion,
    editor.controller,
    repo.loading,
    operations.busy,
    editor.savingPath,
    projectFiles,
    repository?.repositoryId,
    repository?.versions?.files,
    repository?.snapshotId,
    includeIgnored,
  ]);
  editorGate.current = editor;
  const autosave = useEditorAutosave(
    {
      mode: appSettings.saved?.settings.editor.saveMode ?? "manual",
      getSnapshot: () => ({
        documents: editor.controller.getSnapshot().tabs.map((tab) => ({
          path: tab.document.path,
          draft: tab.draft,
          baseline: tab.baseline,
        })),
      }),
      save: editor.controller.save,
      blocked: () =>
        workspace.preview ||
        operations.getSnapshot().busy ||
        syncRemote.busy ||
        Boolean(editor.controller.getSnapshot().savingPath),
    },
    repository?.repositoryId ?? null,
  );
  /** 内容修改经同一草稿控制器，再按设置调度保存。 */
  function editDocument(path: string, content: string): void {
    editor.controller.edit(path, content);
    autosave.notifyChanged(path);
  }
  const [editorAction, setEditorAction] = useState<
    (() => Promise<void>) | null
  >(null);
  const [editorLeaving, setEditorLeaving] = useState(false);
  const [editorReloadPath, setEditorReloadPath] = useState<string | null>(null);
  /** 离开前记录动作，保存失败或继续输入时保留草稿和原项目。 */
  function guardEditor(action: () => Promise<void>): void {
    setEditorReloadPath(null);
    void (async () => {
      if (
        appSettings.saved?.settings.editor.saveMode === "auto" ||
        editor.controller.getSnapshot().savingPath
      )
        await autosave.flush();
      if (editor.controller.getSnapshot().savingPath) return;
      if (editor.controller.dirty()) setEditorAction(() => action);
      else await action();
    })();
  }
  /** 冲突处理后等待实时设置队列，避免刚编辑时旧渲染状态跳过保存。 */
  async function continueExit(action: () => Promise<void>): Promise<void> {
    if (appSettings.saved !== null && !(await appSettings.save())) {
      settingsExit.current = action;
      setSettingsPending("exit");
    } else {
      await workspaceSession.flush();
      await action();
    }
  }
  /** 退出依次经过文件、冲突、设置确认，最后才交给终端结束流程。 */
  function guardExit(action: () => Promise<void>): void {
    if (
      (operations.getSnapshot().busy &&
        !editor.controller.getSnapshot().savingPath) ||
      appSettings.activity === "loading"
    )
      return;
    guardEditor(async () => {
      if (conflicts.dirty) {
        setDiscardAction(() => () => {
          void conflicts.refresh(true).then(() => {
            if (!conflicts.getSnapshot().dirty) void continueExit(action);
          });
        });
      } else await continueExit(action);
    });
  }
  /** 用户取消离开，不改变草稿。 */
  function keepEditor(): void {
    if (!editorLeaving) {
      setEditorAction(null);
      setEditorReloadPath(null);
    }
  }
  /** 只有确认放弃时才关闭所有文档并继续原动作。 */
  async function discardEditor(): Promise<void> {
    if (editorLeaving || editor.savingPath) return;
    if (editorReloadPath) {
      setEditorLeaving(true);
      try {
        if (await editor.controller.reload(editorReloadPath, true)) {
          setEditorAction(null);
          setEditorReloadPath(null);
        }
      } finally {
        setEditorLeaving(false);
      }
      return;
    }
    const action = editorAction;
    setEditorAction(null);
    for (const tab of editor.controller.getSnapshot().tabs)
      editor.controller.close(tab.document.path, true);
    await action?.();
  }
  /** 逐份保存并重签文件身份，任一失败或新草稿阻止离开。 */
  async function saveEditorAndContinue(): Promise<void> {
    if (editorLeaving || editor.savingPath) return;
    setEditorLeaving(true);
    try {
      if (editorReloadPath) {
        if (!(await autosave.retry(editorReloadPath))) return;
      } else await autosave.flush();
      if (
        editorReloadPath
          ? editor.controller
              .getSnapshot()
              .tabs.some(
                (tab) =>
                  tab.document.path === editorReloadPath &&
                  tab.draft !== tab.baseline,
              )
          : editor.controller.dirty()
      )
        return;
      const action = editorAction;
      setEditorAction(null);
      setEditorReloadPath(null);
      await action?.();
    } finally {
      setEditorLeaving(false);
    }
  }
  /** 原生项目选择在处理草稿后执行，不隐式丢弃当前文件。 */
  function openProject(): void {
    workspaceSession.userOpening();
    guardEditor(workspace.open);
  }
  /** 重新检测可能更换 Git 身份，先处理未保存文件。 */
  function refreshGitProtected(): void {
    guardEditor(async () => {
      await workspace.git.refresh();
    });
  }
  /** 原生 Git 路径变更复用同一离开保护。 */
  function chooseGitProtected(): void {
    guardEditor(workspace.chooseGit);
  }
  /** 自动 Git 路径恢复也不能绕过草稿保护。 */
  function resetGitProtected(): void {
    guardEditor(workspace.resetGit);
  }
  /** 编辑标签选择同步文件列表中的可见路径。 */
  function selectEditor(path: string): () => void {
    return () => {
      workspaceSession.userOpening();
      setProjectPath(path);
      void editor.controller.open(path);
    };
  }
  /** 关闭单个文件由控制器提供保存、放弃与取消状态。 */
  function closeEditor(path: string): () => void {
    return () => {
      workspaceSession.userOpening();
      void (async () => {
        if (appSettings.saved?.settings.editor.saveMode === "auto")
          await autosave.retry(path);
        editor.controller.close(path);
      })();
    };
  }
  /** 编辑保存只修改工作文件，不暂存或提交。 */
  function saveEditor(path: string): void {
    void autosave.retry(path);
  }
  /** 重读脏文档前复用统一的保存或放弃确认。 */
  function reloadEditor(): void {
    const path = editor.activePath;
    if (!path) return;
    if (
      editor.tabs.some(
        (tab) => tab.document.path === path && tab.draft !== tab.baseline,
      )
    ) {
      setEditorReloadPath(path);
      setEditorAction(() => async () => {
        if (
          editor.controller
            .getSnapshot()
            .tabs.some((tab) => tab.document.path === path)
        )
          await editor.controller.reload(path);
        else await editor.controller.open(path);
      });
    } else void editor.controller.reload(path);
  }
  const [selected, setSelected] = useState<
    Record<"stage" | "unstage", string[]>
  >({ stage: [], unstage: [] });
  // 暂存或刷新后剔除旧方向的 changeId，避免提交重复携带失效选择。
  useEffect(() => {
    setSelected((current) => reconcileChangeSelection(changeGroups, current));
  }, [changeGroups]);
  const [fields, setFields] = useState<Record<Field, string>>({
    message: "",
    branchName: "",
    cloneUrl: "",
    directoryName: "",
    targetBranch: "",
    mergeMessage: "",
  });
  const [cloneParent, setCloneParent] = useState<CloneParent | null>(null);
  const [parentChoosing, setParentChoosing] = useState(false);
  const [remoteId, setRemoteId] = useState("");
  const [discardAction, setDiscardAction] = useState<(() => void) | null>(null);
  const pendingCommit = useRef<string | null>(null);
  const pendingDiff = useRef<{ path: string; side: DiffSide } | null>(null);
  const resourceGeneration = useRef(0);
  const fileGeneration = useRef(0);
  const alive = useRef(true);
  useEffect(() => {
    if (repository?.rootPath) recent.refresh();
  }, [repository?.rootPath, recent.refresh]);
  /** 项目菜单展示跨启动保存的最近成功打开项目。 */
  function toggleProjectMenu(): void {
    setProjectMenu((value) => !value);
  }
  /** 当前会话项目切换复用后端打开流程并遵守任务与草稿门禁。 */
  function openRecent(path: string): () => void {
    return () => {
      if (workspace.canOpen) {
        workspaceSession.userOpening();
        setProjectMenu(false);
        guardEditor(async () => {
          await repo.open(path);
        });
      }
    };
  }
  /** 折叠分支列表只影响侧栏展示。 */
  function toggleBranches(): void {
    setBranchesExpanded((value) => !value);
  }
  /** 设置分类切换不改变已保存值。 */
  function selectSettings(category: SettingsCategory): () => void {
    return () => setSettingsCategory(category);
  }
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
      fileGeneration.current += 1;
    };
  }, []);
  useEffect(() => {
    resourceGeneration.current += 1;
    fileGeneration.current += 1;
    setBranches(null);
    loadedBranchesKey.current = null;
    setContext(null);
    setProjectFiles(null);
    setProjectTree(null);
    setProjectDiff(null);
    setProjectPath(editor.controller.getSnapshot().activePath ?? "");
    setProjectLoading(false);
    setResourceErrors({});
    setSelected({ stage: [], unstage: [] });
    setChangeDetailOpen(false);
    return () => {
      resourceGeneration.current += 1;
    };
  }, [repository?.repositoryId, workspace.preview]);
  const historyReady = workspace.history.ready;
  const branchesBlocked =
    repo.loading ||
    operations.busy ||
    Boolean(editor.savingPath) ||
    syncRemote.busy ||
    manualRefresh.busy;
  useEffect(() => {
    if (!repository || workspace.preview || !historyReady || branchesBlocked)
      return;
    const key = JSON.stringify([
      repository.repositoryId,
      repository.versions?.history ?? repository.snapshotId,
    ]);
    // 文件保存不改变历史：已经加载的分支无需每次保存后重复查询。
    if (loadedBranchesKey.current === key) return;
    const generation = resourceGeneration.current;
    let active = true;
    const abort = new AbortController();
    void readBranchesWithRetry(
      () => api.readBranches(repository.repositoryId),
      abort.signal,
    )
      .then((value) => {
        if (
          !active ||
          !alive.current ||
          generation !== resourceGeneration.current
        )
          return;
        if (value.repositoryId === repository.repositoryId) {
          loadedBranchesKey.current = key;
          setBranches(value);
          setResourceError(null, "branches");
        }
      })
      .catch((error: unknown) => {
        if (
          active &&
          alive.current &&
          generation === resourceGeneration.current
        )
          // 连续取消已耗尽有限重试，保留明确错误，允许用户手动刷新恢复。
          setResourceErrors((previous) => ({
            ...previous,
            branches: normalizeOperationError(error),
          }));
      });
    return () => {
      active = false;
      abort.abort();
    };
  }, [
    repository?.repositoryId,
    repository?.versions?.history,
    repository?.snapshotId,
    workspace.preview,
    historyReady,
    branchesBlocked,
  ]);
  const needsWriteContext =
    !operations.busy &&
    !editor.savingPath &&
    (drawer === "changes" ||
      drawer === "conflicts" ||
      modal === "branch" ||
      modal === "remote");
  useEffect(() => {
    if (!repository || workspace.preview || !needsWriteContext) return;
    let active = true;
    // 写入能力只在操作面板需要时读取，避免阻塞首屏历史的仓库队列。
    void api
      .readWriteContext(repository.repositoryId, repository.snapshotId)
      .then((value) => {
        if (
          active &&
          value.repositoryId === repository.repositoryId &&
          value.snapshotId === repository.snapshotId
        ) {
          setContext(value);
          setResourceError(null, "context");
        }
      })
      .catch((error: unknown) => {
        if (
          active &&
          normalizeOperationError(error).code !== "REQUEST_CANCELLED"
        )
          setResourceError(normalizeOperationError(error), "context");
      });
    return () => {
      active = false;
    };
  }, [repository, workspace.preview, needsWriteContext]);
  useEffect(() => {
    setFields((old) => ({
      ...old,
      message: "",
      branchName: "",
      mergeMessage: "",
    }));
    setModal(null);
    setProjectMenu(false);
    setDrawer(null);
    setRemoteId("");
    pendingCommit.current = null;
  }, [repository?.repositoryId]);
  useEffect(() => {
    setRemoteId((current) =>
      workspace.remote.remote?.remotes.some(
        (value) => value.remoteId === current,
      )
        ? current
        : "",
    );
  }, [workspace.remote.remote]);
  useEffect(() => {
    if (operations.preview) setModal(null);
  }, [operations.preview]);
  useEffect(() => {
    const done = workspace.completion;
    if (!done) return;
    if (
      done.verified &&
      done.result.outcome === "succeeded" &&
      done.result.kind === "commit"
    ) {
      const submittedMessage = pendingCommit.current;
      setFields((old) =>
        old.message === submittedMessage ? { ...old, message: "" } : old,
      );
      pendingCommit.current = null;
    }
    if (done.result.outcome === "needsResolution") setDrawer("conflicts");
  }, [workspace.completion]);
  useEffect(() => {
    if (operations.failureNotification) setShowOperations(true);
  }, [operations.failureNotification]);
  const blocked =
    operations.busy || syncRemote.busy || repo.loading || repo.stale;
  /** 为禁用按钮附近和辅助技术提供明确的同步门禁原因。 */
  function syncReason(): string {
    if (workspace.preview) return "请在桌面应用中同步远程";
    if (!repository) return "请先打开仓库";
    if (syncRemote.busy) return syncRemote.phaseLabel;
    if (operations.busy || manualRefresh.busy) return "请先等待当前操作完成";
    if (repo.loading || repo.stale) return "请先完成仓库刷新";
    if (conflicts.dirty || editor.dirty) return "请先保存或放弃未保存的草稿";
    if (repository.head.kind !== "branch") return "请切换到已有提交的本地分支";
    if (repository.operations.length) return "请先完成当前合并或其他 Git 操作";
    if (repository.changes.length) return "请先处理工作区、暂存区和未跟踪文件";
    return "";
  }
  const canOpenWrite =
    Boolean(repository) &&
    !workspace.preview &&
    !blocked &&
    !conflicts.dirty &&
    !editor.dirty;
  const canSwitchBranch =
    canOpenWrite &&
    (!context || context.capabilities.switchBranch.status === "allowed");
  /** 能力与当前快照一致才开放写入口，详细拒绝原因保留在后端预览。 */
  function canWrite(
    kind: Exclude<OperationKind, "clone" | "saveFile" | "setUpstream">,
  ): boolean {
    return (
      !blocked &&
      !conflicts.dirty &&
      !editor.dirty &&
      context?.capabilities[
        kind === "commit" && selected.stage.length > 0 ? "commitSelected" : kind
      ]?.status === "allowed"
    );
  }
  /** 提供按钮和读屏可使用的能力门禁原因。 */
  function writeReason(
    kind: Exclude<OperationKind, "clone" | "saveFile" | "setUpstream">,
  ): string {
    if (workspace.preview) return "请在桌面应用中使用 Git 功能";
    if (!repository) return "请先打开仓库";
    if (operations.busy) return "请先完成当前确认或任务";
    if (conflicts.dirty) return "请先保存或放弃冲突草稿";
    if (editor.dirty) return "请先保存或放弃文件草稿";
    if (repo.loading || repo.stale) return "请等待仓库刷新成功";
    const capability =
      context?.capabilities[
        kind === "commit" && selected.stage.length > 0 ? "commitSelected" : kind
      ];
    return !capability
      ? "正在读取写入能力"
      : capability.status === "error"
        ? describeGitError(capability.error)
        : "";
  }
  /** 切换视图前显式保留或放弃冲突草稿。 */
  function navigate(next: Drawer): void {
    workspaceSession.userOpening();
    if (conflicts.dirty && next !== "conflicts") {
      setDiscardAction(() => () => {
        void conflicts.refresh(true);
        setDrawer(next);
      });
      return;
    }
    setDrawer(next);
  }
  /** 为展示组件提供不含业务判断的导航回调。 */
  function openDrawer(next: Drawer): () => void {
    return () => navigate(next);
  }
  /** 弹窗只在无任务和无草稿时打开。 */
  function openModal(next: Modal): () => void {
    return () => {
      if (
        next === "settings" ||
        (!operations.busy && !conflicts.dirty && !editor.dirty)
      ) {
        setProjectMenu(false);
        setModal(next);
      }
    };
  }
  /** 关闭表单，不取消任何后台任务。 */
  function closeModal(): void {
    if (modal === "settings") {
      if (appSettings.activity === "loading") return;
      void appSettings.save().then((saved) => {
        if (saved) setModal(null);
        else setSettingsPending("close");
      });
      return;
    }
    if (operations.activity === "preparing") operations.discardPreview();
    setModal(null);
  }
  /** 在不改变输入值的情况下关闭抽屉。 */
  function closeDrawer(): void {
    navigate(null);
  }
  /** 切换操作记录面板。 */
  function toggleOperations(): void {
    setShowOperations((value) => !value);
  }
  /** 统一表单的受控文本更新。 */
  function field(
    name: Field,
  ): (event: ChangeEvent<HTMLInputElement | HTMLTextAreaElement>) => void {
    return (event) =>
      setFields((old) => ({ ...old, [name]: event.target.value }));
  }
  /** 勾选只决定写入范围，不改变当前查看的文件。 */
  function toggleChange(kind: "stage" | "unstage", id: string): () => void {
    return () =>
      setSelected((old) => ({
        ...old,
        [kind]: old[kind].includes(id)
          ? old[kind].filter((value) => value !== id)
          : [...old[kind], id],
      }));
  }
  /** 分组全选覆盖虚拟列表外文件，不改变其他组勾选。 */
  function toggleChangeGroup(side: DiffSide): () => void {
    return () => {
      if (blocked) return;
      const action = side === "staged" ? "unstage" : "stage";
      setSelected((old) => ({
        ...old,
        [action]: toggleGroupSelection(changeGroups[side], old[action]),
      }));
    };
  }
  /** 查看单文件差异，不隐式勾选或暂存。 */
  function inspectChange(id: string, side: DiffSide): () => void {
    return () => {
      setChangeDetailOpen(true);
      if (
        editor.controller.getSnapshot().savingPath ||
        operations.getSnapshot().busy
      ) {
        const change = repo
          .getSnapshot()
          .repository?.changes.find((item) => item.changeId === id);
        pendingDiff.current = change ? { path: change.path, side } : null;
        return;
      }
      void repo.selectDiff(id, side);
    };
  }
  /** 返回文件列表时保留勾选与提交说明。 */
  function closeChangeDetail(): void {
    pendingDiff.current = null;
    setChangeDetailOpen(false);
    repo.closeDiff();
  }
  /** 按档位补读当前文件上下文，沿用差异请求代次和后端内容上限。 */
  function expandChangeContext(): void {
    const selected = repo.selected;
    if (
      !selected ||
      selected.side === "untracked" ||
      repo.diffLoading ||
      (repo.diff?.kind !== "text" && repo.diff?.kind !== "paged") ||
      repo.diff.truncated
    )
      return;
    const current = selected.contextLines ?? 3;
    if (current >= 2000) return;
    void repo.selectDiff(
      selected.changeId,
      selected.side,
      Math.min(2000, Math.max(20, current * 5)),
    );
  }
  const selection = selectionAction(selected.stage, selected.unstage);
  /** 合并按钮只能提交唯一方向，混合选择即使程序调用也拒绝。 */
  function prepareSelected(): void {
    if (selection.kind === "stage" || selection.kind === "unstage")
      prepareSelection(selection.kind)();
  }
  /** 选中项进入后端精确路径预览。 */
  function prepareSelection(kind: "stage" | "unstage"): () => void {
    return () => {
      if (canWrite(kind))
        void operations.runLocal({ kind, changeIds: selected[kind] });
    };
  }
  /** 提交确认整个已暂存索引，成功后才清除这次提交说明。 */
  function prepareCommit(): void {
    if (!canWrite("commit")) return;
    pendingCommit.current = fields.message;
    void operations.runLocal({
      kind: "commit",
      message: fields.message,
      stageChangeIds: selected.stage,
    });
  }
  /** 创建分支只创建引用，不切换工作区。 */
  function prepareBranch(): void {
    if (canWrite("createBranch"))
      void operations.runLocal({
        kind: "createBranch",
        name: fields.branchName,
      });
  }
  /** 切换仅接受后端分支 ID；按需请求预览，由后端完整校验并等待用户确认。 */
  function switchBranch(id: string): () => void {
    return () => {
      if (canSwitchBranch)
        void operations.prepareLocal({ kind: "switchBranch", branchId: id });
    };
  }
  /** 原生父目录选择器取消时保留上次选择。 */
  async function chooseParent(): Promise<void> {
    if (parentChoosing || operations.busy) return;
    setParentChoosing(true);
    setResourceError(null);
    try {
      const value = await api.chooseCloneParent();
      if (alive.current && value) setCloneParent(value);
    } catch (error: unknown) {
      if (alive.current) setResourceError(normalizeOperationError(error));
    } finally {
      if (alive.current) setParentChoosing(false);
    }
  }
  /** 克隆只提交目录身份和新目录名，目标路径由后端验证。 */
  function prepareClone(): void {
    if (cloneParent && !operations.busy)
      void operations.prepareClone({
        url: fields.cloneUrl,
        parentDirectoryId: cloneParent.parentDirectoryId,
        directoryName: fields.directoryName,
      });
  }
  /** 更换远端使旧分支评估失效，不发起网络访问。 */
  function changeRemote(event: ChangeEvent<HTMLSelectElement>): void {
    setRemoteId(event.target.value);
    void workspace.remote.selectBranch("");
  }
  /** 选择跟踪分支只比较已有本地对象。 */
  function changeRemoteBranch(event: ChangeEvent<HTMLSelectElement>): void {
    void workspace.remote.selectBranch(event.target.value);
  }
  /** fetch 只更新指定跟踪引用，不整合工作区。 */
  function prepareFetch(): void {
    if (canWrite("fetch") && workspace.remote.selectedBranchId)
      void operations.prepareRemote({
        kind: "fetch",
        remoteId,
        remoteBranchId: workspace.remote.selectedBranchId,
      });
  }
  /** 推送预览会访问服务器核对目标并可能触发系统认证。 */
  function preparePush(): void {
    if (canWrite("push"))
      void operations.prepareRemote({
        kind: "push",
        remoteId,
        targetBranchName: fields.targetBranch,
      });
  }
  /** 明确选定整合模式，不隐式执行 pull 或 rebase。 */
  function prepareIntegration(mode: "fastForward" | "merge"): () => void {
    return () => {
      if (canWrite("integrate") && workspace.remote.selectedBranchId)
        void operations.prepareRemote({
          kind: "integrate",
          remoteBranchId: workspace.remote.selectedBranchId,
          mode,
        });
    };
  }
  /** 选择冲突文件，未保存草稿必须先确认放弃。 */
  function selectConflict(id: string): () => void {
    return () => {
      if (conflicts.dirty) {
        setDiscardAction(() => () => {
          void conflicts.selectFile(id, true);
        });
        return;
      }
      void conflicts.selectFile(id);
    };
  }
  /** 修改结果区仅保存在内存草稿。 */
  function editConflict(event: ChangeEvent<HTMLTextAreaElement>): void {
    conflicts.edit(event.target.value);
  }
  /** 采用一侧只复制原始文本到草稿，不写文件。 */
  function useConflictSide(side: "local" | "incoming"): () => void {
    return () => {
      const value = conflicts.document?.[side];
      if (value !== null && value !== undefined) conflicts.edit(value);
    };
  }
  /** 将带原始指纹的当前草稿交给后端预览。 */
  function prepareSaveConflict(): void {
    const request = conflicts.saveRequest();
    const session = conflicts.conflicts?.mergeSessionId;
    if (request && session && !operations.busy)
      void operations.prepareConflict(session, request);
  }
  /** 完成合并必须重新验证整个索引和双亲。 */
  function prepareFinishMerge(): void {
    const session = conflicts.conflicts?.mergeSessionId;
    if (session && canWrite("finishMerge"))
      void operations.prepareConflict(session, {
        kind: "finishMerge",
        message: fields.mergeMessage,
      });
  }
  /** 刷新冲突文件前先处理未保存草稿。 */
  function refreshConflicts(): void {
    if (conflicts.dirty) {
      setDiscardAction(() => () => {
        void conflicts.refresh(true);
      });
      return;
    }
    void conflicts.refresh();
  }
  /** 用户确认丢弃后才执行原导航。 */
  function discardDraft(): void {
    const action = discardAction;
    setDiscardAction(null);
    action?.();
  }
  /** 保留草稿并取消待执行导航。 */
  function keepDraft(): void {
    setDiscardAction(null);
  }
  /** 展开提交详情时保留真实图选择。 */
  async function selectCommit(oid: string): Promise<void> {
    navigate("detail");
    await workspace.history.selectCommit(oid);
  }
  /** 选择直接父提交，根提交维持空树比较。 */
  function changeParent(event: ChangeEvent<HTMLSelectElement>): void {
    void workspace.history.selectParent(event.target.value || null);
  }
  /** 展示签发文件 ID 对应的历史差异。 */
  function inspectCommitFile(id: string): () => void {
    return () => {
      void workspace.history.selectFile(id);
    };
  }
  /** 列表重新签发后同步更新控制器引用，多文档连续保存不使用旧 ID。 */
  async function reloadProjectFiles(): Promise<void> {
    const current = repo.getSnapshot().repository;
    if (!current || workspace.preview) return;
    const key = JSON.stringify([
      current.repositoryId,
      current.versions?.files ?? current.snapshotId,
      includeIgnored,
    ]);
    if (listRequest.current?.key === key) return listRequest.current.promise;
    if (treeCurrent.current && loadedTreeKey.current === key) return;
    setProjectLoading(true);
    const promise = api
      .readProjectTree(current.repositoryId, current.snapshotId, includeIgnored)
      .then(async (value) => {
        let files = mergeTreeFiles(null, value);
        // 已打开的深层文档保存前重签身份，不要求用户重新展开所有祖先。
        for (const tab of editorGate.current?.controller.getSnapshot().tabs ??
          []) {
          const path = tab.document.path;
          if (files.files.some((file) => file.path === path)) continue;
          let parent = value;
          const parts = path.split("/");
          for (let index = 0; index < parts.length; index++) {
            let page = parent;
            let entry = page.entries.find((item) => item.name === parts[index]);
            while (!entry && page.nextOffset !== null) {
              if (!alive.current || listRequest.current?.key !== key) return;
              page = await api.readProjectDirectory(
                value.repositoryId,
                value.snapshotId,
                value.treeId,
                page.directoryId,
                page.nextOffset,
              );
              entry = page.entries.find((item) => item.name === parts[index]);
            }
            if (!entry) break;
            if (index === parts.length - 1) {
              if (entry.kind === "file")
                files = mergeTreeFiles(files, { ...page, entries: [entry] });
            } else if (entry.kind === "directory") {
              parent = await api.readProjectDirectory(
                value.repositoryId,
                value.snapshotId,
                value.treeId,
                entry.id,
                0,
              );
            } else break;
          }
        }
        if (
          alive.current &&
          listRequest.current?.key === key &&
          repo.getSnapshot().repository?.repositoryId ===
            current.repositoryId &&
          (repo.getSnapshot().repository?.versions?.files ??
            repo.getSnapshot().repository?.snapshotId) ===
            (current.versions?.files ?? current.snapshotId)
        ) {
          treeCurrent.current = value;
          loadedTreeKey.current = key;
          setProjectTree(value);
          fileList.current = files;
          setProjectFiles(files);
          setResourceError(null, "files");
        }
      })
      .catch((error: unknown) => {
        if (alive.current && listRequest.current?.key === key)
          setResourceError(normalizeOperationError(error), "files");
      })
      .finally(() => {
        if (alive.current && listRequest.current?.key === key) {
          setProjectLoading(false);
          listRequest.current = null;
        }
      });
    listRequest.current = { key, promise };
    await promise;
  }
  /** 安装目录页的文件能力前确认当前树未被刷新替换。 */
  function acceptTreePage(page: ProjectTreePage): ProjectTreePage {
    if (
      treeCurrent.current?.treeId !== page.treeId ||
      repo.getSnapshot().repository?.repositoryId !== page.repositoryId
    )
      throw { code: "STALE_REQUEST" };
    const files = mergeTreeFiles(fileList.current, page);
    fileList.current = files;
    setProjectFiles(files);
    return page;
  }
  /** 按路径恢复标签，所有读取都使用新树签发的文件身份。 */
  async function restoreWorkspace(
    session: WorkspaceSession,
    valid: () => boolean,
  ): Promise<void> {
    await reloadProjectFiles();
    const root = treeCurrent.current;
    if (!root || !valid()) return;
    const current = (): boolean =>
      valid() && treeCurrent.current?.treeId === root.treeId;
    const parents = session.documents.map((item) =>
      item.path.split("/").slice(0, -1).join("/"),
    );
    const pages = await restoreTreePaths(
      root,
      parents,
      loadProjectDirectory,
      current,
    );
    let missing = false;
    for (const item of session.documents) {
      if (!current()) return;
      const parentPath = item.path.split("/").slice(0, -1).join("/");
      const directoryId = parentPath
        ? Object.values(pages)
            .flatMap((page) => page.entries)
            .find(
              (entry) =>
                entry.kind === "directory" && entry.path === parentPath,
            )?.id
        : root.directoryId;
      let page = directoryId ? pages[directoryId] : undefined;
      if (!page) {
        missing = true;
        continue;
      }
      while (
        !page.entries.some((entry) => entry.path === item.path) &&
        page.nextOffset !== null
      ) {
        const next = await loadProjectDirectory(
          page.directoryId,
          page.nextOffset,
        );
        if (!current()) return;
        page = { ...next, entries: [...page.entries, ...next.entries] };
        pages[page.directoryId] = page;
      }
      acceptTreePage(page);
      if (
        !page.entries.some(
          (entry) => entry.kind === "file" && entry.path === item.path,
        )
      ) {
        missing = true;
        continue;
      }
      await editor.controller.open(item.path);
    }
    if (!current()) return;
    if (
      session.activePath &&
      editor.controller
        .getSnapshot()
        .tabs.some((tab) => tab.document.path === session.activePath)
    )
      await editor.controller.open(session.activePath);
    if (!current()) return;
    setDrawer(session.activePage === "history" ? null : session.activePage);
    if (missing) throw { code: "FILE_UNAVAILABLE" };
  }
  /** 展开和翻页复用当前文件会话，不作废其他文档 ID。 */
  async function loadProjectDirectory(
    directoryId: string,
    offset: number,
  ): Promise<ProjectTreePage> {
    const tree = treeCurrent.current;
    if (!tree) throw { code: "STALE_REQUEST" };
    return acceptTreePage(
      await api.readProjectDirectory(
        tree.repositoryId,
        tree.snapshotId,
        tree.treeId,
        directoryId,
        offset,
      ),
    );
  }
  /** 路径筛选在后端清单执行，只把命中页面交给视图。 */
  async function searchProjectFiles(
    query: string,
    offset: number,
  ): Promise<ProjectTreePage> {
    const tree = treeCurrent.current;
    if (!tree) throw { code: "STALE_REQUEST" };
    return acceptTreePage(
      await api.searchProjectFiles(
        tree.repositoryId,
        tree.snapshotId,
        tree.treeId,
        query,
        offset,
      ),
    );
  }
  useEffect(() => {
    if (
      (drawer !== "files" && !editor.tabs.length) ||
      !repository ||
      workspace.preview
    )
      return;
    void reloadProjectFiles();
  }, [
    drawer,
    repository,
    workspace.preview,
    includeIgnored,
    editor.tabs.length > 0,
  ]);
  /** 仅改变项目文件可见范围，不修改忽略规则或文件内容。 */
  function changeIncludeIgnored(event: ChangeEvent<HTMLInputElement>): void {
    setIncludeIgnored(event.target.checked);
  }
  /** 只预览当前签发列表中的项目文件。 */
  function inspectProjectFile(id: string): () => void {
    return () => {
      workspaceSession.userOpening();
      const file = projectFiles?.files.find((value) => value.fileId === id);
      if (!file || !repository) return;
      fileGeneration.current += 1;
      setProjectPath(file.path);
      void editor.controller.open(file.path);
      setProjectDiff(null);
      setProjectLoading(false);
    };
  }
  /** 关闭确认只放弃当前待关闭文档。 */
  function discardEditorTab(): void {
    const path = editor.controller.getSnapshot().pendingClose;
    if (path) editor.controller.close(path, true);
  }
  /** 关闭确认复用串行保存队列，避免与自动保存并发写入。 */
  async function saveEditorTabAndClose(): Promise<void> {
    const path = editor.controller.getSnapshot().pendingClose;
    if (path && (await autosave.retry(path))) editor.controller.close(path);
  }
  /** 保存工具栏当前文档。 */
  function saveActiveEditor(): void {
    if (editor.activePath) saveEditor(editor.activePath);
  }
  useEffect(() => {
    setProjectPath(editor.activePath ?? "");
  }, [editor.activePath]);
  useEffect(() => {
    const code = editor.error?.code;
    if (
      !projectPath ||
      !repository ||
      !projectFiles ||
      ![
        "FILE_EDIT_BINARY",
        "FILE_EDIT_ENCODING",
        "FILE_EDIT_UNSUPPORTED",
      ].includes(code ?? "")
    )
      return;
    const file = projectFiles.files.find((item) => item.path === projectPath);
    if (!file) return;
    let aliveFallback = true;
    const generation = fileGeneration.current;
    const fileSnapshot = projectFiles.snapshotId;
    setProjectLoading(true);
    void api
      .readProjectFile(
        repository.repositoryId,
        projectFiles.snapshotId,
        file.fileId,
      )
      .then((value) => {
        if (
          aliveFallback &&
          generation === fileGeneration.current &&
          fileList.current?.snapshotId === fileSnapshot
        ) {
          setProjectDiff(value);
          setResourceError(null, "preview");
        }
      })
      .catch((error: unknown) => {
        if (aliveFallback)
          setResourceError(normalizeOperationError(error), "preview");
      })
      .finally(() => {
        if (aliveFallback) setProjectLoading(false);
      });
    return () => {
      aliveFallback = false;
    };
  }, [editor.error?.code, projectPath, projectFiles, repository]);
  const workspaceSession = useWorkspaceSession({
    enabled: !workspace.preview,
    ready: workspace.git.environment?.status === "ready" && !repo.loading,
    repository,
    paths: editor.tabs.map((tab) => tab.document.path),
    activePath: editor.activePath,
    page: drawer === "files" || drawer === "changes" ? drawer : "history",
    open: repo.open,
    restore: restoreWorkspace,
  });
  useEffect(() => {
    const pending = pendingDiff.current;
    if (!pending || operations.busy || editor.savingPath || repo.loading)
      return;
    pendingDiff.current = null;
    if (drawer !== "changes") return;
    const change = repository?.changes.find(
      (item) => item.path === pending.path,
    );
    if (change) void repo.selectDiff(change.changeId, pending.side);
    else setChangeDetailOpen(false);
  }, [
    operations.busy,
    editor.savingPath,
    repo.loading,
    repository,
    drawer,
    repo.selectDiff,
  ]);
  /** 显式本地核实保留文档草稿，不触发远端获取。 */
  async function refreshLocal(): Promise<void> {
    if (
      editor.controller.getSnapshot().savingPath ||
      operations.getSnapshot().busy ||
      syncRemote.busy ||
      manualRefresh.busy
    )
      return;
    const previous = repo.getSnapshot().repository;
    await repo.refresh();
    await reloadProjectFiles();
    const current = repo.getSnapshot();
    // 新历史版本会自动重读远端；同版本重试才补读，避免两个入口重复排队。
    if (
      current.repository &&
      !current.loading &&
      !current.error &&
      previous?.repositoryId === current.repository.repositoryId &&
      previous?.versions?.history === current.repository.versions?.history
    )
      await workspace.remote.refresh();
  }
  useWorkbenchShortcuts(
    {
      togglePanel: toggleOperations,
      save: saveActiveEditor,
      openProject,
      openSettings: openModal("settings"),
      dismiss: () => {
        if (projectMenu) setProjectMenu(false);
        else if (changeDetailOpen) closeChangeDetail();
        else if (drawer) closeDrawer();
        else return false;
        return true;
      },
      focusSearch: () =>
        document
          .querySelector<HTMLInputElement>(".graph-search input")
          ?.focus(),
    },
    (event) => {
      if (
        document.querySelector("dialog[open]") ||
        modal ||
        editorAction ||
        settingsPending
      )
        return false;
      const target = event.target instanceof Element ? event.target : null;
      const key = event.key.toLowerCase();
      if (target?.closest(".monaco-editor") && (key === "s" || key === "f"))
        return false;
      if (target?.closest(".xterm") && key !== "j") return false;
      if (key === "o" && !workspace.canOpen) return false;
      return true;
    },
  );
  return {
    ...workspace,
    git: { ...workspace.git, refresh: refreshGitProtected },
    chooseGit: chooseGitProtected,
    resetGit: resetGitProtected,
    open: openProject,
    editor,
    workspaceSession,
    refreshLocal,
    editDocument,
    editorCompositionStart: autosave.compositionStart,
    editorCompositionEnd: autosave.compositionEnd,
    editorPending: editorAction !== null,
    editorLeaving,
    editorReloadPath,
    keepEditor,
    discardEditor,
    saveEditorAndContinue,
    selectEditor,
    closeEditor,
    saveEditor,
    discardEditorTab,
    saveEditorTabAndClose,
    saveActiveEditor,
    reloadEditor,
    guardEditor,
    guardExit,
    exitProtected:
      editor.dirty ||
      Boolean(editor.savingPath) ||
      conflicts.dirty ||
      appSettings.dirty ||
      appSettings.activity !== "idle" ||
      operations.busy,
    manualRefresh,
    syncRemote,
    syncReason,
    branchesExpanded,
    projectMenu,
    recentProjects: recent.projects,
    refreshRecentProjects: recent.refresh,
    recentProjectsError: recent.error
      ? "最近项目记录读取或保存失败，请重试。"
      : null,
    settingsCategory,
    toggleProjectMenu,
    openRecent,
    toggleBranches,
    selectSettings,
    preferences,
    appSettings,
    settingsPending,
    applySettings,
    restoreSettings,
    reloadSettings,
    keepSettings,
    discardSettings,
    saveSettingsAndContinue,
    logSettings,
    drawer,
    modal,
    showOperations,
    branches,
    context,
    resourceError,
    projectFiles,
    projectTree,
    loadProjectDirectory,
    searchProjectFiles,
    includeIgnored,
    changeIncludeIgnored,
    projectDiff,
    projectPath,
    projectLoading,
    selected,
    selection,
    prepareSelected,
    changeDetailOpen,
    changeDetailFocus,
    closeChangeDetail,
    expandChangeContext,
    fields,
    cloneParent,
    parentChoosing,
    remoteId,
    discardPending: discardAction !== null,
    blocked,
    groups: changeGroups,
    canWrite,
    canOpenWrite,
    canSwitchBranch,
    writeReason,
    openDrawer,
    openModal,
    closeModal,
    closeDrawer,
    toggleOperations,
    field,
    toggleChange,
    toggleChangeGroup,
    inspectChange,
    prepareSelection,
    prepareCommit,
    prepareBranch,
    switchBranch,
    chooseParent,
    prepareClone,
    changeRemote,
    changeRemoteBranch,
    prepareFetch,
    preparePush,
    prepareIntegration,
    selectConflict,
    editConflict,
    useConflictSide,
    prepareSaveConflict,
    prepareFinishMerge,
    refreshConflicts,
    discardDraft,
    keepDraft,
    selectCommit,
    changeParent,
    inspectCommitFile,
    inspectProjectFile,
  };
}
export type Workbench = ReturnType<typeof useWorkbench>;
