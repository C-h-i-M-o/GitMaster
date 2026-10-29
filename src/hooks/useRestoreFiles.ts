import {
  useEffect,
  useRef,
  useState,
  useSyncExternalStore,
  type ChangeEvent,
} from "react";
import { createRestoreController, type RestoreApi } from "./restoreController";

/** 将恢复来源、候选分页与确认预览接入 React，业务状态仍由可测试控制器持有。 */
export function useRestoreFiles(
  api: RestoreApi,
  snapshotId: string | undefined,
) {
  const current = useRef(api);
  current.current = api;
  const [controller] = useState(() =>
    createRestoreController({
      repository: () => current.current.repository(),
      blocked: () => current.current.blocked(),
      read: (...args) => current.current.read(...args),
      prepare: (request) => current.current.prepare(request),
    }),
  );
  const state = useSyncExternalStore(
    controller.subscribe,
    controller.getSnapshot,
  );
  useEffect(() => {
    controller.close();
    return controller.close;
  }, [controller, snapshotId]);
  /** 来源变化立即撤销旧文件选择，提交 OID 由后端检查完整格式。 */
  function changeSource(event: ChangeEvent<HTMLSelectElement>): void {
    const kind = event.target.value;
    if (kind === "index" || kind === "head") controller.selectSource({ kind });
    else controller.selectSource({ kind: "commit", oid: "" });
  }
  /** 保留用户输入原文，不将任意 revspec 扩展为受支持来源。 */
  function changeOid(event: ChangeEvent<HTMLInputElement>): void {
    controller.selectSource({ kind: "commit", oid: event.target.value });
  }
  /** JSX 只绑定已经限定文件身份的勾选处理器。 */
  function toggle(id: string): () => void {
    return () => controller.toggle(id);
  }
  /** 用户主动重读来源，清除上一轮选择。 */
  function reload(): void {
    void controller.load();
  }
  /** 后续页保留同一来源已经勾选的身份。 */
  function next(): void {
    void controller.load(true);
  }
  return {
    ...state,
    openDialog: controller.open,
    close: controller.close,
    prepare: controller.prepare,
    toggle,
    changeSource,
    changeOid,
    reload,
    next,
  };
}
