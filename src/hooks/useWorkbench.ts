import { useEffect, useRef, useState, type ChangeEvent } from "react";
import { useWorkspace } from "./useWorkspace";
import { usePreferences } from "./usePreferences";
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
  const preferences = usePreferences(!workspace.preview);
  const [drawer, setDrawer] = useState<Drawer>(null);
  const [modal, setModal] = useState<Modal>(null);
  const [showOperations, setShowOperations] = useState(false);
  const [branchesExpanded, setBranchesExpanded] = useState(true);
  const [projectMenu, setProjectMenu] = useState(false);
  const [recentProjects, setRecentProjects] = useState<string[]>([]);
  const [settingsCategory, setSettingsCategory] = useState<
    "general" | "appearance"
  >("general");
  const [branches, setBranches] = useState<BranchList | null>(null);
  const [context, setContext] = useState<WriteContext | null>(null);
  const [resourceError, setResourceError] = useState<OperationError | null>(
    null,
  );
  const [projectFiles, setProjectFiles] = useState<ProjectFileList | null>(
    null,
  );
  const [projectDiff, setProjectDiff] = useState<FileDiff | null>(null);
  const [projectPath, setProjectPath] = useState("");
  const [projectLoading, setProjectLoading] = useState(false);
  const [selected, setSelected] = useState<
    Record<"stage" | "unstage", string[]>
  >({ stage: [], unstage: [] });
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
  const resourceGeneration = useRef(0);
  const fileGeneration = useRef(0);
  const alive = useRef(true);
  useEffect(() => {
    if (repository?.rootPath)
      setRecentProjects((old) =>
        [
          repository.rootPath,
          ...old.filter((path) => path !== repository.rootPath),
        ].slice(0, 10),
      );
  }, [repository?.rootPath]);
  /** 项目菜单只列出当前会话真实打开成功的路径。 */
  function toggleProjectMenu(): void {
    setProjectMenu((value) => !value);
  }
  /** 当前会话项目切换复用后端打开流程并遵守任务与草稿门禁。 */
  function openRecent(path: string): () => void {
    return () => {
      if (workspace.canOpen) {
        setProjectMenu(false);
        void repo.open(path);
      }
    };
  }
  /** 折叠分支列表只影响侧栏展示。 */
  function toggleBranches(): void {
    setBranchesExpanded((value) => !value);
  }
  /** 设置分类切换不改变已保存值。 */
  function selectSettings(category: "general" | "appearance"): () => void {
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
    const generation = ++resourceGeneration.current;
    fileGeneration.current += 1;
    setBranches(null);
    setContext(null);
    setProjectFiles(null);
    setProjectDiff(null);
    setProjectPath("");
    setProjectLoading(false);
    setResourceError(null);
    setSelected({ stage: [], unstage: [] });
    if (!repository || workspace.preview) return;
    void Promise.allSettled([
      api.readBranches(repository.repositoryId),
      api.readWriteContext(repository.repositoryId, repository.snapshotId),
    ]).then(([branchResult, contextResult]) => {
      if (!alive.current || generation !== resourceGeneration.current) return;
      if (
        branchResult.status === "fulfilled" &&
        branchResult.value.repositoryId === repository.repositoryId
      )
        setBranches(branchResult.value);
      else if (branchResult.status === "rejected")
        setResourceError(normalizeOperationError(branchResult.reason));
      if (
        contextResult.status === "fulfilled" &&
        contextResult.value.repositoryId === repository.repositoryId &&
        contextResult.value.snapshotId === repository.snapshotId
      )
        setContext(contextResult.value);
      else if (contextResult.status === "rejected")
        setResourceError(normalizeOperationError(contextResult.reason));
    });
    return () => {
      resourceGeneration.current += 1;
    };
  }, [repository, workspace.preview]);
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
    setShowOperations(true);
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
  const blocked = operations.busy || repo.loading || repo.stale;
  /** 能力与当前快照一致才开放写入口，详细拒绝原因保留在后端预览。 */
  function canWrite(kind: Exclude<OperationKind, "clone">): boolean {
    return (
      !blocked &&
      !conflicts.dirty &&
      context?.capabilities[kind].status === "allowed"
    );
  }
  /** 提供按钮和读屏可使用的能力门禁原因。 */
  function writeReason(kind: Exclude<OperationKind, "clone">): string {
    if (workspace.preview) return "请在桌面应用中使用 Git 功能";
    if (!repository) return "请先打开仓库";
    if (operations.busy) return "请先完成当前确认或任务";
    if (conflicts.dirty) return "请先保存或放弃冲突草稿";
    if (repo.loading || repo.stale) return "请等待仓库刷新成功";
    const capability = context?.capabilities[kind];
    return !capability
      ? "正在读取写入能力"
      : capability.status === "error"
        ? describeGitError(capability.error)
        : "";
  }
  /** 切换视图前显式保留或放弃冲突草稿。 */
  function navigate(next: Drawer): void {
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
      if (!operations.busy && !conflicts.dirty) {
        preferences.resetDraft();
        setProjectMenu(false);
        setModal(next);
      }
    };
  }
  /** 关闭表单，不取消任何后台任务。 */
  function closeModal(): void {
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
  /** 查看单文件差异，不隐式勾选或暂存。 */
  function inspectChange(id: string, side: DiffSide): () => void {
    return () => {
      void repo.selectDiff(id, side);
    };
  }
  /** 选中项进入后端精确路径预览。 */
  function prepareSelection(kind: "stage" | "unstage"): () => void {
    return () => {
      if (canWrite(kind))
        void operations.prepareLocal({ kind, changeIds: selected[kind] });
    };
  }
  /** 提交确认整个已暂存索引，成功后才清除这次提交说明。 */
  function prepareCommit(): void {
    if (!canWrite("commit")) return;
    pendingCommit.current = fields.message;
    void operations.prepareLocal({ kind: "commit", message: fields.message });
  }
  /** 创建分支只创建引用，不切换工作区。 */
  function prepareBranch(): void {
    if (canWrite("createBranch"))
      void operations.prepareLocal({
        kind: "createBranch",
        name: fields.branchName,
      });
  }
  /** 切换仅接受后端签发的本地分支 ID。 */
  function switchBranch(id: string): () => void {
    return () => {
      if (canWrite("switchBranch"))
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
  useEffect(() => {
    if (drawer !== "files" || !repository || workspace.preview) return;
    const generation = ++fileGeneration.current;
    setProjectLoading(true);
    setProjectFiles(null);
    setProjectDiff(null);
    void api
      .readProjectFiles(repository.repositoryId, repository.snapshotId)
      .then((value) => {
        if (alive.current && generation === fileGeneration.current)
          setProjectFiles(value);
      })
      .catch((error: unknown) => {
        if (alive.current && generation === fileGeneration.current)
          setResourceError(normalizeOperationError(error));
      })
      .finally(() => {
        if (alive.current && generation === fileGeneration.current)
          setProjectLoading(false);
      });
    return () => {
      fileGeneration.current += 1;
    };
  }, [drawer, repository, workspace.preview]);
  /** 只预览当前签发列表中的项目文件。 */
  function inspectProjectFile(id: string): () => void {
    return () => {
      const file = projectFiles?.files.find((value) => value.fileId === id);
      if (!file || !repository) return;
      const generation = ++fileGeneration.current;
      setProjectPath(file.path);
      setProjectDiff(null);
      setProjectLoading(true);
      void api
        .readProjectFile(repository.repositoryId, repository.snapshotId, id)
        .then((value) => {
          if (alive.current && generation === fileGeneration.current)
            setProjectDiff(value);
        })
        .catch((error: unknown) => {
          if (alive.current && generation === fileGeneration.current)
            setResourceError(normalizeOperationError(error));
        })
        .finally(() => {
          if (alive.current && generation === fileGeneration.current)
            setProjectLoading(false);
        });
    };
  }
  return {
    ...workspace,
    branchesExpanded,
    projectMenu,
    recentProjects,
    settingsCategory,
    toggleProjectMenu,
    openRecent,
    toggleBranches,
    selectSettings,
    preferences,
    drawer,
    modal,
    showOperations,
    branches,
    context,
    resourceError,
    projectFiles,
    projectDiff,
    projectPath,
    projectLoading,
    selected,
    fields,
    cloneParent,
    parentChoosing,
    remoteId,
    discardPending: discardAction !== null,
    blocked,
    groups: groupChanges(repository?.changes ?? []),
    canWrite,
    writeReason,
    openDrawer,
    openModal,
    closeModal,
    closeDrawer,
    toggleOperations,
    field,
    toggleChange,
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
