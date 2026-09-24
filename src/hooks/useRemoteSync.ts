import {
  useEffect,
  useRef,
  useState,
  useSyncExternalStore,
  type ChangeEvent,
} from "react";
import {
  createRemoteSyncController,
  type RemoteSyncApi,
} from "./remoteSyncController";

/** 将同步状态机与目标表单接入 React，仓库切换仅终止后续编排。 */
export function useRemoteSync(
  api: RemoteSyncApi,
  repositoryId: string | undefined,
) {
  const current = useRef(api);
  current.current = api;
  const [controller] = useState(() =>
    createRemoteSyncController({
      repository: () => current.current.repository(),
      blocked: () => current.current.blocked(),
      readRemotes: (id) => current.current.readRemotes(id),
      readSyncTarget: (id, snapshot) =>
        current.current.readSyncTarget(id, snapshot),
      assessRemote: (id, snapshot, branch) =>
        current.current.assessRemote(id, snapshot, branch),
      execute: (repository, request) =>
        current.current.execute(repository, request),
    }),
  );
  const state = useSyncExternalStore(
    controller.subscribe,
    controller.getSnapshot,
  );
  const [remoteId, setRemoteId] = useState("");
  const [branchName, setBranchName] = useState("");
  const [allowCreate, setAllowCreate] = useState(false);
  useEffect(() => {
    controller.activate();
    return () => controller.deactivate();
  }, [controller]);
  useEffect(() => {
    controller.reset();
  }, [controller, repositoryId]);
  useEffect(() => {
    const setup = state.setup;
    setRemoteId(
      setup?.target.upstream.status === "configured"
        ? setup.target.upstream.remoteId
        : "",
    );
    setBranchName(
      setup?.target.upstream.status === "configured"
        ? setup.target.upstream.targetBranchName
        : (setup?.target.branchName ?? ""),
    );
    setAllowCreate(false);
  }, [state.setup]);
  /** 选择用户明确指定的远端，不根据名称猜测 origin。 */
  function selectRemote(event: ChangeEvent<HTMLSelectElement>): void {
    setRemoteId(event.target.value);
  }
  /** 编辑远端目标分支名称，最终格式校验由 Git 完成。 */
  function editBranch(event: ChangeEvent<HTMLInputElement>): void {
    setBranchName(event.target.value);
  }
  /** 新建远端分支须由用户明确允许。 */
  function changeCreate(event: ChangeEvent<HTMLInputElement>): void {
    setAllowCreate(event.target.checked);
  }
  /** 表单明确范围后执行设置和同步，不增加低风险重复确认。 */
  async function confirm(): Promise<void> {
    await controller.configure(remoteId, branchName, allowCreate);
  }
  const phaseLabel = {
    idle: "同步远程",
    checking: "检查上游…",
    fetching: "获取更新…",
    assessing: "比较分支…",
    syncing: "正在同步…",
  }[state.phase];
  const setupWarning =
    state.setup?.target.upstream.status === "unresolved"
      ? "当前上游配置无法唯一识别。确认后将使用下方选择作为当前分支的上游。"
      : null;
  const suggestions =
    state.setup?.remotes.remotes.find((remote) => remote.remoteId === remoteId)
      ?.fetchedBranchNames ?? [];
  return {
    ...state,
    remoteId,
    branchName,
    allowCreate,
    phaseLabel,
    setupWarning,
    suggestions,
    selectRemote,
    editBranch,
    changeCreate,
    confirm,
    run: controller.run,
    closeSetup: controller.closeSetup,
  };
}
