import { useEffect, useMemo, useState } from "react";
import type { Workbench } from "./useWorkbench";
import { workspaceNotices, workbenchNoticeInput } from "../ui/workspaceNotices";

/** 成功反馈五秒后收起，错误继续保留；每个项目分别管理通知生命周期。 */
export function useWorkspaceNotices(w: Workbench) {
  const [expired, setExpired] = useState<ReadonlySet<string>>(new Set());
  const notices = workspaceNotices(workbenchNoticeInput(w));
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
