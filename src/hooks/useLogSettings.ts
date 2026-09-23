import { useEffect, useRef, useState, type ChangeEvent } from "react";
import {
  readLogSettings,
  setLogLevel,
  openLogDirectory,
} from "../services/git";
import { normalizeOperationError } from "../services/gitErrors";
import type { LogLevel, LogSettings, OperationError } from "../types/git";

/** 管理日志设置的读取、即时保存与卸载后的结果隔离。 */
export function useLogSettings(enabled: boolean) {
  const [settings, setSettings] = useState<LogSettings | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<OperationError | null>(null);
  const generation = useRef(0);
  const pending = useRef(false);
  const active = useRef(enabled);
  active.current = enabled;

  /** 串行执行设置操作，仅发布当前组件仍有效的响应。 */
  async function run(action: () => Promise<LogSettings | void>): Promise<void> {
    if (!active.current || pending.current) return;
    const token = generation.current;
    pending.current = true;
    setBusy(true);
    setError(null);
    try {
      const result = await action();
      if (token === generation.current && result) setSettings(result);
    } catch (cause: unknown) {
      if (token === generation.current)
        setError(normalizeOperationError(cause));
    } finally {
      if (token === generation.current) {
        pending.current = false;
        setBusy(false);
      }
    }
  }
  /** 重试读取真实持久化设置。 */
  function reload(): void {
    void run(readLogSettings);
  }
  /** 验证选项并立即保存，失败时保持原选择。 */
  function change(event: ChangeEvent<HTMLSelectElement>): void {
    const value = event.target.value;
    const levels: readonly string[] = [
      "error",
      "warn",
      "info",
      "debug",
      "trace",
    ];
    if (value !== "auto" && !levels.includes(value)) return;
    void run(() => setLogLevel(value === "auto" ? null : (value as LogLevel)));
  }
  /** 通过受限后端接口打开应用日志目录。 */
  function openDirectory(): void {
    void run(openLogDirectory);
  }
  /** 桌面启用后加载，卸载时丢弃在途响应。 */
  useEffect(() => {
    active.current = enabled;
    if (enabled) reload();
    return () => {
      generation.current += 1;
      pending.current = false;
      active.current = false;
    };
  }, [enabled]);
  return { settings, busy, error, reload, change, openDirectory };
}
