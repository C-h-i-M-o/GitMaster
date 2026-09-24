import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import * as git from "../services/git";
import type { RepositoryState } from "../types/git";
import {
  createOperationsController,
  type CompletionHandler,
} from "./operationsController";
/** 将任务控制器接入窗口生命周期，重新挂载仅恢复查询。 */
export function useOperations(
  repository: RepositoryState | null,
  enabled: boolean,
  onComplete: CompletionHandler,
) {
  const [controller] = useState(() => createOperationsController(git));
  const callback = useRef(onComplete);
  useEffect(() => {
    callback.current = onComplete;
  }, [onComplete]);
  const state = useSyncExternalStore(
    controller.subscribe,
    controller.getSnapshot,
  );
  useEffect(() => {
    controller.setCompletionHandler((result, scope) =>
      callback.current(result, scope),
    );
  }, [controller]);
  useEffect(() => {
    controller.setRepository(repository, enabled);
  }, [controller, repository, enabled]);
  useEffect(() => {
    controller.activate();
    if (enabled) void controller.resume();
    return () => controller.deactivate();
  }, [controller, enabled]);
  return {
    ...state,
    getSnapshot: controller.getSnapshot,
    prepareLocal: controller.prepareLocal,
    runLocal: controller.runLocal,
    saveFile: controller.saveFile,
    runFileSave: controller.runFileSave,
    prepareRemote: controller.prepareRemote,
    runRemote: controller.runRemote,
    fetchAll: controller.fetchAll,
    prepareConflict: controller.prepareConflict,
    prepareClone: controller.prepareClone,
    confirm: controller.confirm,
    discardPreview: controller.discardPreview,
    resume: controller.resume,
  };
}
