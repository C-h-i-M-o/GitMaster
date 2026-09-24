import { useEffect, useRef, useState, type ChangeEvent } from "react";
import { readRemotes } from "../services/git";
import { normalizeOperationError } from "../services/gitErrors";
import type { OperationsViewState } from "./operationsController";
import type { RepositoryViewState } from "./repositoryController";
import {
  fetchStatus,
  localRefreshStatus,
  refreshStatusLabel,
  type RefreshStatus,
} from "../ui/refreshResult";
import type {
  OperationError,
  OperationResult,
  RemoteWriteRequest,
  RemoteState,
  RepositoryState,
} from "../types/git";

/** 手动刷新先选择当前分支对应远端；内部状态刷新仍不访问网络。 */
export function useManualRefresh(
  localView: RepositoryViewState,
  blocked: boolean,
  execute: (
    repository: RepositoryState,
    request: RemoteWriteRequest,
  ) => Promise<OperationResult>,
  localRefresh: () => Promise<void>,
  getLocalView: () => RepositoryViewState,
  getOperation: () => OperationsViewState,
) {
  const repository = localView.repository;
  const [busy, setBusy] = useState(false);
  const [choices, setChoices] = useState<RemoteState | null>(null);
  const [selected, setSelected] = useState("");
  const [error, setError] = useState<OperationError | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [networkStatus, setNetworkStatus] = useState<RefreshStatus>("idle");
  const [localStatus, setLocalStatus] = useState<RefreshStatus>("idle");
  const pending = useRef(false);
  const generation = useRef(0);
  const current = useRef({ repository, blocked });
  current.current = { repository, blocked };
  useEffect(() => {
    ++generation.current;
    pending.current = false;
    setBusy(false);
    setChoices(null);
    setSelected("");
    setError(null);
    setNotice(null);
    setNetworkStatus("idle");
    setLocalStatus("idle");
    return () => {
      ++generation.current;
    };
  }, [repository?.repositoryId]);
  useEffect(() => {
    setChoices(null);
    setSelected("");
  }, [repository?.snapshotId]);
  useEffect(() => {
    if (localStatus === "running" && repository && !localView.loading)
      setLocalStatus(localRefreshStatus(localView, repository.repositoryId));
  }, [localView, localStatus, repository]);
  /** 直接刷新不吞掉本地错误，也不把排队中的读取描述为成功。 */
  async function refreshLocal(token: number): Promise<void> {
    const id = repository?.repositoryId;
    if (!id || token !== generation.current) return;
    setLocalStatus("running");
    await localRefresh();
    if (token === generation.current)
      setLocalStatus(localRefreshStatus(getLocalView(), id));
  }
  /** 等待本次获取终态及本地回调，网络与本地结果分开保留。 */
  async function fetchAndRefresh(id: string, token: number): Promise<void> {
    if (!repository || token !== generation.current) return;
    setNetworkStatus("running");
    try {
      const result = await execute(repository, {
        kind: "fetchAll",
        remoteId: id,
      });
      if (token !== generation.current) return;
      setNetworkStatus(fetchStatus(result));
      setLocalStatus(
        localRefreshStatus(getLocalView(), repository.repositoryId),
      );
      if (result.outcome === "failed" || result.outcome === "unknown")
        setError(result.error);
    } catch (cause: unknown) {
      if (token !== generation.current) return;
      const error = normalizeOperationError(cause);
      const uncertain =
        getOperation().busy || error.code === "WRITE_OUTCOME_UNKNOWN";
      setError(error);
      setNetworkStatus(uncertain ? "unverified" : "failed");
      if (!uncertain) await refreshLocal(token);
    }
  }
  /** 未唯一确定远端时由用户选择，不猜测 origin。 */
  async function run(): Promise<void> {
    if (!repository || current.current.blocked || pending.current) return;
    const token = generation.current;
    pending.current = true;
    setBusy(true);
    setError(null);
    setNotice(null);
    setNetworkStatus("idle");
    setLocalStatus("idle");
    try {
      const remotes = await readRemotes(repository.repositoryId);
      if (
        token !== generation.current ||
        current.current.blocked ||
        repository.snapshotId !== current.current.repository?.snapshotId
      )
        return;
      if (!remotes.remotes.length) {
        setNotice("未配置远端，仅刷新本地。");
        setNetworkStatus("skipped");
        await refreshLocal(token);
        return;
      }
      const branchName =
        repository.head.kind === "branch" ? repository.head.name : null;
      const upstream = remotes.branchUpstreams.find(
        (item) => item.branchName === branchName,
      );
      const id =
        upstream?.remoteId ??
        (!upstream && remotes.remotes.length === 1
          ? remotes.remotes[0]?.remoteId
          : undefined);
      if (id) await fetchAndRefresh(id, token);
      else {
        setChoices(remotes);
        setSelected("");
      }
    } catch (cause: unknown) {
      if (token === generation.current) {
        setError(normalizeOperationError(cause));
        setNetworkStatus("failed");
        if (!current.current.blocked) await refreshLocal(token);
      }
    } finally {
      if (token === generation.current) {
        pending.current = false;
        setBusy(false);
      }
    }
  }
  /** 只选择本轮列表签发的远端身份。 */
  function select(event: ChangeEvent<HTMLSelectElement>): void {
    setSelected(event.target.value);
  }
  /** 取消选择不修改仓库或远端。 */
  function cancel(): void {
    if (!pending.current) setChoices(null);
  }
  /** 明确目标后执行全分支获取，后端仍复核会话与配置。 */
  async function confirm(): Promise<void> {
    if (
      current.current.blocked ||
      pending.current ||
      !choices?.remotes.some((remote) => remote.remoteId === selected)
    )
      return;
    const token = generation.current;
    pending.current = true;
    setBusy(true);
    setError(null);
    setChoices(null);
    try {
      await fetchAndRefresh(selected, token);
    } catch (cause: unknown) {
      if (generation.current === token)
        setError(normalizeOperationError(cause));
    } finally {
      if (generation.current === token) {
        pending.current = false;
        setBusy(false);
      }
    }
  }
  return {
    run,
    busy,
    error,
    notice,
    resultLabel:
      networkStatus !== "idle" || localStatus !== "idle"
        ? `远端获取：${refreshStatusLabel(networkStatus)} · 本地读取：${refreshStatusLabel(localStatus)}`
        : null,
    choices,
    selected,
    select,
    cancel,
    confirm,
  };
}
