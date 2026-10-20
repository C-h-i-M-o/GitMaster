import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import {
  isDesktop,
  readCommitDetail,
  readCommitFileDiff,
  readCommitFiles,
  readCommitHistory,
} from "../services/git";
import type { RepositoryState } from "../types/git";
import { createHistoryController } from "./historyController";
export type { HistoryApi, HistoryState } from "./historyController";
/** 将历史控制器接入 React；浏览器预览不触发 IPC。 */
export function useHistory(
  repository: RepositoryState | null,
  enabled: boolean,
) {
  const [controller] = useState(() =>
    createHistoryController({
      readCommitHistory,
      readCommitDetail,
      readCommitFiles,
      readCommitFileDiff,
    }),
  );
  const state = useSyncExternalStore(
    controller.subscribe,
    controller.getSnapshot,
  );
  const requested = useRef<RepositoryState | null>(null);
  useEffect(() => {
    requested.current = enabled ? repository : null;
    controller.activate();
    controller.setRepository(requested.current);
    if (enabled && isDesktop()) void controller.refresh();
    return () => controller.deactivate();
  }, [
    controller,
    repository?.repositoryId,
    repository?.versions?.history,
    enabled,
  ]);
  return {
    ...state,
    // 新仓库版本的首轮渲染仍可能持有旧 page，不能提前放行依赖它的后台读取。
    ready:
      enabled &&
      repository !== null &&
      requested.current?.repositoryId === repository.repositoryId &&
      requested.current?.versions?.history === repository.versions?.history &&
      Boolean(state.page || state.error),
    refresh: controller.refresh,
    loadMore: controller.loadMore,
    selectCommit: controller.selectCommit,
    selectParent: controller.selectParent,
    selectFile: controller.selectFile,
  };
}
