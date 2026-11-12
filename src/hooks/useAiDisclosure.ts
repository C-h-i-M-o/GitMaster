import { useState, type SyntheticEvent } from "react";
/** 自动规则变化时重置展开状态，用户点击仅影响当前可见层级。 */
export function useAiDisclosure(initialOpen: boolean, resetVersion: string) {
  const [manual, setManual] = useState<{
    version: string;
    open: boolean;
  } | null>(null);
  // 偏好切换后清除旧手动状态，切回同一选项时不能恢复过期展开状态。
  if (manual && manual.version !== resetVersion) setManual(null);
  const open = manual?.version === resetVersion ? manual.open : initialOpen;
  /** 原生 details 同时支持鼠标与键盘，禁止直接操作 DOM 之外的会话状态。 */
  function toggle(event: SyntheticEvent<HTMLDetailsElement>): void {
    if (event.currentTarget.open !== open)
      setManual({ version: resetVersion, open: event.currentTarget.open });
  }
  return { open, toggle };
}
