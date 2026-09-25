import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import {
  createFileEditorController,
  type FileEditorApi,
} from "./fileEditorController";

/** 将已验证的草稿控制器接入工作台，仓库刷新不清除同仓库文档。 */
export function useFileEditor(api: FileEditorApi, repositoryId: string | null) {
  const current = useRef(api);
  current.current = api;
  const [controller] = useState(() =>
    createFileEditorController({
      repository: () => current.current.repository(),
      files: () => current.current.files(),
      blocked: () => current.current.blocked(),
      read: (repository, id) => current.current.read(repository, id),
      save: (repository, document, content) =>
        current.current.save(repository, document, content),
    }),
  );
  const state = useSyncExternalStore(
    controller.subscribe,
    controller.getSnapshot,
  );
  useEffect(() => {
    controller.setRepository(repositoryId);
  }, [controller, repositoryId]);
  return { ...state, controller, dirty: controller.dirty() };
}
