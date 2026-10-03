import { useEffect, useMemo, useSyncExternalStore } from "react";
import {
  EditorAutosaveController,
  type EditorAutosaveControllerOptions,
} from "./editorAutosaveController";

/** 将保存队列绑定到仓库会话；切项目时先释放旧队列再安装新队列。 */
export function useEditorAutosave(
  options: EditorAutosaveControllerOptions,
  repositoryId: string | null,
): EditorAutosaveController {
  const instance = useMemo(
    () => new EditorAutosaveController(options),
    [repositoryId],
  );
  instance.updateOptions(options);
  useEffect(() => {
    instance.setMode(options.mode ?? "manual");
  }, [instance, options.mode]);
  useEffect(() => {
    instance.activate();
    return () => instance.dispose();
  }, [instance]);
  useSyncExternalStore(
    instance.subscribe,
    instance.getSnapshot,
    instance.getSnapshot,
  );
  return instance;
}
