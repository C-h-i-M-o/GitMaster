import { useEffect, useMemo, useState } from "react";
import type { Workbench } from "./useWorkbench";
import { workspaceNotices } from "../ui/workspaceNotices";

/** 成功反馈五秒后收起，错误继续保留；每个项目分别管理通知生命周期。 */
export function useWorkspaceNotices(w: Workbench) {
  const [expired, setExpired] = useState<ReadonlySet<string>>(new Set());
  const localFileOperation =
    w.operations.operationScope?.kind === "saveFile" ||
    w.operations.operationScope?.kind === "saveConflict";
  const foreignOperation =
    w.operations.operationScope?.repositoryId != null &&
    w.operations.operationScope.repositoryId !==
      w.repo.repository?.repositoryId;
  const notices = workspaceNotices({
    sync: w.syncRemote,
    refresh: w.manualRefresh,
    errors: [
      w.actionError,
      w.drawer === "files" ? null : w.resourceError,
      w.repo.error,
      localFileOperation || foreignOperation ? null : w.operations.error,
    ],
    stale: w.repo.stale,
    activity:
      localFileOperation || foreignOperation ? "idle" : w.operations.activity,
  });
  const successKey = notices
    .filter((notice) => notice.tone === "success")
    .map((notice) => notice.id)
    .join("\n");
  useEffect(() => {
    setExpired(new Set());
  }, [
    w.repo.repository?.repositoryId,
    w.syncRemote.busy,
    w.manualRefresh.busy,
  ]);
  useEffect(() => {
    if (!successKey) return;
    const timer = setTimeout(
      () => setExpired((old) => new Set([...old, ...successKey.split("\n")])),
      5000,
    );
    return () => clearTimeout(timer);
  }, [successKey, w.repo.repository?.repositoryId]);
  return useMemo(
    () =>
      notices.filter(
        (notice) => notice.tone !== "success" || !expired.has(notice.id),
      ),
    [notices, expired],
  );
}
