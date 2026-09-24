import { useEffect, useRef, useState, type ChangeEvent } from "react";
import {
  openProjectFolder,
  readExternalAvailability,
  type ExternalAppId,
} from "../services/git";
import { normalizeOperationError } from "../services/gitErrors";
import type { OperationError } from "../types/git";

/** 外部打开按当前仓库身份执行，离开面板后丢弃迟到反馈。 */
export function useProjectFolder(
  repositoryId: string | undefined,
  enabled: boolean,
  defaultApp: ExternalAppId,
) {
  const [selected, setSelected] = useState<ExternalAppId>(defaultApp);
  const [availability, setAvailability] = useState<{
    vsCode: boolean;
    terminal: boolean;
  } | null>(null);
  useEffect(() => {
    setSelected(defaultApp);
  }, [defaultApp]);
  useEffect(() => {
    let current = true;
    if (enabled)
      void readExternalAvailability()
        .then((value) => {
          if (current) setAvailability(value);
        })
        .catch((cause: unknown) => {
          if (current) setError(normalizeOperationError(cause));
        });
    return () => {
      current = false;
    };
  }, [enabled]);
  /** 切换本次打开方式不自动改写全局默认设置。 */
  function select(event: ChangeEvent<HTMLSelectElement>): void {
    const value = event.target.value;
    if (value === "fileManager" || value === "vsCode" || value === "terminal")
      setSelected(value);
  }
  const available =
    selected === "fileManager" || availability?.[selected] === true;
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<OperationError | null>(null);
  const active = useRef(true);
  const pending = useRef(false);
  useEffect(() => {
    active.current = true;
    return () => {
      active.current = false;
    };
  }, []);
  /** 明确点击后才请求系统打开，拒绝快速重复启动。 */
  async function open(): Promise<void> {
    if (!enabled || !repositoryId || !available || pending.current) return;
    pending.current = true;
    setBusy(true);
    setError(null);
    try {
      await openProjectFolder(repositoryId, selected);
    } catch (cause: unknown) {
      if (active.current) setError(normalizeOperationError(cause));
    } finally {
      pending.current = false;
      if (active.current) setBusy(false);
    }
  }
  return { open, busy, error, selected, select, available, availability };
}
