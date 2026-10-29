import { useCallback, useEffect, useRef, useState } from "react";
import { clearContentCache, readCacheUsage } from "../services/settings";
import type { CacheUsage } from "../types/settings";

/** 缓存面板按需读取用量；离开或更新设置后丢弃晚到结果。 */
export function useContentCache(
  enabled: boolean,
  revision: string | undefined,
) {
  const [usage, setUsage] = useState<CacheUsage | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const generation = useRef(0);
  const pending = useRef(false);
  const active = useRef(enabled);
  active.current = enabled;

  /** 操作不自动重试；失败后保留明确提示，避免把未知用量显示为零。 */
  const run = useCallback(async (clear: boolean): Promise<void> => {
    if (!active.current || pending.current) return;
    pending.current = true;
    const token = ++generation.current;
    setBusy(true);
    setError(null);
    try {
      const result = await (clear ? clearContentCache() : readCacheUsage());
      if (token === generation.current) setUsage(result);
    } catch {
      if (token === generation.current) {
        setUsage(null);
        setError(
          clear
            ? "缓存未能清除，请稍后重试。项目文件、草稿和设置不会被清除。"
            : "暂时无法读取缓存用量。缓存不可用时会直接读取 Git，请稍后重试。",
        );
      }
    } finally {
      if (token === generation.current) {
        pending.current = false;
        setBusy(false);
      }
    }
  }, []);

  useEffect(() => {
    pending.current = false;
    setUsage(null);
    setBusy(false);
    setError(null);
    if (enabled) void run(false);
    return () => {
      ++generation.current;
    };
  }, [enabled, revision, run]);

  /** 用户主动刷新当前用量。 */
  function reload(): void {
    void run(false);
  }
  /** 只请求清除后端固定的专用缓存目录。 */
  function clear(): void {
    void run(true);
  }
  return { usage, busy, error, reload, clear };
}

/** 用十进制单位对应 1GB 的产品配额，始终保留精确字节值。 */
export function cacheSize(bytes: number): string {
  return `${(bytes / 1_000_000).toFixed(2)} MB（${bytes.toLocaleString("zh-CN")} 字节）`;
}
