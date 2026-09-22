import { useEffect, useState, useSyncExternalStore } from "react";
import { readConflictDocument, readConflicts } from "../services/git";
import type { RepositoryState } from "../types/git";
import { createConflictsController } from "./conflictsController";
/** 每个工作区独立持有冲突草稿，浏览器预览不发起读取。 */
export function useConflicts(
  repository: RepositoryState | null,
  enabled: boolean,
) {
  const [controller] = useState(() =>
    createConflictsController({ readConflicts, readConflictDocument }),
  );
  const state = useSyncExternalStore(
    controller.subscribe,
    controller.getSnapshot,
  );
  useEffect(() => {
    controller.activate();
    controller.setRepository(enabled ? repository : null);
    if (enabled && repository?.operations.includes("merge"))
      void controller.refresh();
    return () => controller.deactivate();
  }, [controller, repository, enabled]);
  return {
    ...state,
    refresh: controller.refresh,
    selectFile: controller.selectFile,
    edit: controller.edit,
    acknowledgeSave: controller.acknowledgeSave,
    saveRequest: controller.saveRequest,
  };
}
