import { useCallback, useEffect, useRef, useState } from "react";
import { clearContentCache, readCacheUsage } from "../services/settings";
import type { CacheUsage } from "../types/settings";

/** 缓存面板按需读取用量；离开或更新设置后丢弃晚到结果。 */
export function useContentCache(enabled: boolean) {
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
    /** 进入面板及窗口激活时更新真实用量，无需额外刷新按钮。 */
    const refresh = (): void => {
      if (enabled) void run(false);
    };
    refresh();
    window.addEventListener("focus", refresh);
    return () => {
      ++generation.current;
      window.removeEventListener("focus", refresh);
    };
  }, [enabled, run]);

  /** 只请求清除后端固定的专用缓存目录。 */
  function clear(): void {
    void run(true);
  }
  return { usage, busy, error, clear };
}

/** 按实际大小选择十进制单位，零和小缓存也保留可辨识的数值。 */
export function cacheSize(bytes: number): string {
  const units = ["B", "KB", "MB", "GB", "TB"];
  const index = Math.min(
    bytes > 0 ? Math.floor(Math.log10(bytes) / 3) : 0,
    units.length - 1,
  );
  return `${(bytes / 1000 ** index).toLocaleString("zh-CN", { maximumFractionDigits: index === 0 ? 0 : 2 })} ${units[index]}`;
}

/** 用量条显示已占空间的构成，不暗示容量上限。 */
export function cacheContentWidth(usage: CacheUsage): { width: string } {
  return {
    width: `${usage.diskBytes === 0 ? 0 : (usage.contentBytes / usage.diskBytes) * 100}%`,
  };
}
