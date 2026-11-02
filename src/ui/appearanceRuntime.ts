import type { UiPreferences } from "../types/git.ts";
import {
  appearancePalette,
  resolveAppearance,
  type ResolvedAppearance,
} from "./themes.ts";

const eventName = "gitmaster:appearance";
let active = resolveAppearance(
  { colorMode: "light", lightTheme: "github", darkTheme: "github" },
  false,
);

/** 延迟创建的编辑器和终端使用当前已生效主题。 */
export function activeAppearance(): ResolvedAppearance {
  return active;
}

/** 原子更新根节点语义颜色，再通知独立画布控件。 */
export function applyAppearance(
  preferences: UiPreferences,
  systemDark: boolean,
): void {
  const next = resolveAppearance(preferences, systemDark);
  const changed =
    active.mode !== next.mode || active.theme.id !== next.theme.id;
  active = next;
  const root = document.documentElement;
  root.dataset.colorMode = next.mode;
  root.dataset.theme = next.theme.id;
  root.style.colorScheme = next.mode;
  for (const [key, value] of Object.entries(appearancePalette(next)))
    root.style.setProperty(key, value);
  if (changed) window.dispatchEvent(new Event(eventName));
}

/** 订阅配色变化并提供对称清理，避免标签卸载后遗留监听。 */
export function onAppearanceChange(listener: () => void): () => void {
  window.addEventListener(eventName, listener);
  return () => window.removeEventListener(eventName, listener);
}
