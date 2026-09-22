import { useEffect, useState, useSyncExternalStore } from "react";
import {
  openRepository,
  readFileDiff,
  readRepositoryState,
  isDesktop,
} from "../services/git";
import { createRepositoryController } from "./repositoryController";
export type { RepositoryViewState } from "./repositoryController";
/** 将可测试的仓库控制器接入 React 和原生窗口激活事件。 */
export function useRepository() {
  const [controller] = useState(() =>
    createRepositoryController({
      openRepository,
      readFileDiff,
      readRepositoryState,
    }),
  );
  const state = useSyncExternalStore(
    controller.subscribe,
    controller.getSnapshot,
  );
  useEffect(() => {
    controller.activate();
    /** 窗口激活时请求一次合并刷新。 */
    const onFocus = (): void => {
      if (isDesktop()) void controller.refreshAutomatic();
    };
    window.addEventListener("focus", onFocus);
    return () => {
      window.removeEventListener("focus", onFocus);
      controller.deactivate();
    };
  }, [controller]);
  return {
    ...state,
    open: controller.open,
    refresh: controller.refresh,
    setAutoRefreshBlocked: controller.setAutoRefreshBlocked,
    selectDiff: controller.selectDiff,
    clear: controller.clear,
  };
}
