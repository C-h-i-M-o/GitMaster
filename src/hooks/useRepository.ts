import { useEffect, useState, useSyncExternalStore } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  openRepository,
  readRepositoryState,
  readRepositoryWatch,
  isDesktop,
} from "../services/git";
import { createRepositoryController } from "./repositoryController";
import { openDiffDocument, closeDiffDocument } from "../services/diff";
export type { RepositoryViewState } from "./repositoryController";
/** 将可测试的仓库控制器接入 React 和原生窗口激活事件。 */
export function useRepository() {
  const [controller] = useState(() =>
    createRepositoryController({
      openRepository,
      readFileDiff: openDiffDocument,
      closeFileDiff: closeDiffDocument,
      readRepositoryState,
    }),
  );
  const state = useSyncExternalStore(
    controller.subscribe,
    controller.getSnapshot,
  );
  useEffect(() => {
    controller.activate();
    return () => controller.deactivate();
  }, [controller]);
  useEffect(() => {
    let disposed = false;
    let debounceTimer: number | undefined;
    let maxTimer: number | undefined;
    let unreliableTimer: number | undefined;
    const repositoryId = state.repository?.repositoryId;
    let revision = 0;
    let listenerFailed = false;
    /** 拒绝卸载后或切换项目后才返回的监听结果。 */
    const isCurrent = (): boolean =>
      !disposed &&
      controller.getSnapshot().repository?.repositoryId === repositoryId;
    /** 启动低频降级核实，避免不可靠监视状态长期不刷新。 */
    const ensureUnreliableTimer = (): void => {
      if (unreliableTimer === undefined)
        unreliableTimer = window.setInterval(() => {
          if (isCurrent() && document.visibilityState === "visible") {
            controller.markDirty();
            void controller.refreshAutomatic();
          }
        }, 60000);
    };
    /** 在短时间内合并多个监视事件，再由控制器消费 dirty。 */
    const scheduleRefresh = (): void => {
      if (debounceTimer !== undefined) window.clearTimeout(debounceTimer);
      debounceTimer = window.setTimeout(() => {
        debounceTimer = undefined;
        if (maxTimer !== undefined) window.clearTimeout(maxTimer);
        maxTimer = undefined;
        if (document.visibilityState === "visible")
          void controller.refreshAutomatic();
      }, 500);
      if (maxTimer === undefined)
        maxTimer = window.setTimeout(() => {
          maxTimer = undefined;
          if (debounceTimer !== undefined) window.clearTimeout(debounceTimer);
          debounceTimer = undefined;
          if (document.visibilityState === "visible")
            void controller.refreshAutomatic();
        }, 2000);
    };
    /** 合并单调版本，可靠性丢失时开启低频核实。 */
    const applyWatch = (watch: {
      repositoryId: string;
      revision: number;
      reliable: boolean;
    }): void => {
      if (
        !isCurrent() ||
        watch.repositoryId !== repositoryId ||
        watch.revision < revision
      )
        return;
      const changed = watch.revision > revision;
      if (changed) {
        revision = watch.revision;
        controller.markDirty();
        scheduleRefresh();
      }
      if (watch.reliable && !listenerFailed && unreliableTimer !== undefined) {
        window.clearInterval(unreliableTimer);
        unreliableTimer = undefined;
      } else if (!watch.reliable || listenerFailed) {
        ensureUnreliableTimer();
      }
    };
    /** 监听或版本读取失败后保留脏标记，并限制后续核实频率。 */
    const monitoringFailed = (): void => {
      if (!isCurrent()) return;
      ensureUnreliableTimer();
      controller.markDirty();
      scheduleRefresh();
    };
    /** 窗口激活只读取监视状态，不直接读取 Git。 */
    const onFocus = (): void => {
      if (isDesktop() && repositoryId && document.visibilityState === "visible")
        void readRepositoryWatch(repositoryId)
          .then((watch) => {
            applyWatch(watch);
            if (isCurrent()) scheduleRefresh();
          })
          .catch(monitoringFailed);
    };
    window.addEventListener("focus", onFocus);
    /** 隐藏期间只保留变更，恢复可见后再核实。 */
    const onVisibility = (): void => {
      controller.setAutomaticVisible(document.visibilityState === "visible");
      if (document.visibilityState === "visible") onFocus();
    };
    controller.setAutomaticVisible(document.visibilityState === "visible");
    document.addEventListener("visibilitychange", onVisibility);
    const unlistenPromise =
      isDesktop() && repositoryId
        ? listen<{ repositoryId: string; revision: number; reliable: boolean }>(
            "repository-invalidated",
            (event) => {
              if (!disposed && event.payload.repositoryId === repositoryId)
                applyWatch(event.payload);
            },
          ).catch(() => {
            listenerFailed = true;
            monitoringFailed();
            return () => undefined;
          })
        : Promise.resolve(() => undefined);
    if (isDesktop() && repositoryId && document.visibilityState === "visible") {
      void unlistenPromise.then(() => {
        if (isCurrent()) onFocus();
      });
    }
    return () => {
      disposed = true;
      window.removeEventListener("focus", onFocus);
      document.removeEventListener("visibilitychange", onVisibility);
      if (debounceTimer !== undefined) window.clearTimeout(debounceTimer);
      if (maxTimer !== undefined) window.clearTimeout(maxTimer);
      if (unreliableTimer !== undefined) window.clearInterval(unreliableTimer);
      void unlistenPromise.then((unlisten) => unlisten());
    };
  }, [controller, state.repository?.repositoryId]);
  return {
    ...state,
    open: controller.open,
    refresh: controller.refresh,
    getSnapshot: controller.getSnapshot,
    setAutoRefreshBlocked: controller.setAutoRefreshBlocked,
    selectDiff: controller.selectDiff,
    closeDiff: controller.closeDiff,
    clear: controller.clear,
    markDirty: controller.markDirty,
  };
}
