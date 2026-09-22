import { useEffect, useState, useSyncExternalStore } from "react";
import { readRemotes, assessRemote } from "../services/git";
import type { RepositoryState } from "../types/git";
import { createRemoteController } from "./remoteController";
/** 把本地远端评估接入 React，仓库快照变化时重新签发选择列表。 */
export function useRemote(
  repository: RepositoryState | null,
  enabled: boolean,
) {
  const [controller] = useState(() =>
    createRemoteController({ readRemotes, assessRemote }),
  );
  const state = useSyncExternalStore(
    controller.subscribe,
    controller.getSnapshot,
  );
  useEffect(() => {
    controller.activate();
    controller.setRepository(enabled ? repository : null);
    if (enabled) void controller.refresh();
    return () => controller.deactivate();
  }, [controller, repository, enabled]);
  return {
    ...state,
    refresh: controller.refresh,
    selectBranch: controller.selectBranch,
  };
}
