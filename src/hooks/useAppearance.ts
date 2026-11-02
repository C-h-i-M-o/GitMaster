import { useLayoutEffect, useEffect } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { UiPreferences } from "../types/git";
import { applyAppearance } from "../ui/appearanceRuntime";

/** 模式与系统监听共同驱动外观，不改写用户保存的深浅主题选择。 */
export function useAppearance(
  preferences: UiPreferences,
  desktop: boolean,
): void {
  const { colorMode, lightTheme, darkTheme } = preferences;
  useLayoutEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    /** 系统变化只通过解析影响实际模式，手动模式保持原选择。 */
    const update = (): void =>
      applyAppearance(
        { colorMode, lightTheme, darkTheme, elasticity: 6, showLabels: true },
        media.matches,
      );
    update();
    media.addEventListener("change", update);
    return () => media.removeEventListener("change", update);
  }, [colorMode, lightTheme, darkTheme]);
  useEffect(() => {
    if (!desktop) return;
    // 系统模式清除窗口强制外观，避免覆盖系统颜色变化通知。
    void getCurrentWindow()
      .setTheme(colorMode === "system" ? null : colorMode)
      .catch(() => {
        // 原生窗口主题不可用时，WebView 内部配色仍保持已选值。
      });
  }, [colorMode, desktop]);
}
