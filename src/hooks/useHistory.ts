import { useEffect, useState, useSyncExternalStore } from "react";
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
  useEffect(() => {
    controller.activate();
    controller.setRepository(enabled ? repository : null);
    if (enabled && isDesktop()) void controller.refresh();
    return () => controller.deactivate();
  }, [controller, repository, enabled]);
  return {
    ...state,
    refresh: controller.refresh,
    loadMore: controller.loadMore,
    selectCommit: controller.selectCommit,
    selectParent: controller.selectParent,
    selectFile: controller.selectFile,
  };
}
