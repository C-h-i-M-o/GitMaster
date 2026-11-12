import { useEffect, useState } from "react";
import { onAppearanceChange } from "../ui/appearanceRuntime";
/** AI 使用上游深浅色板，模式跟随应用而不修改 Git 主题。 */
export function useAiTheme(): boolean {
  const [dark, setDark] = useState(
    document.documentElement.dataset.colorMode === "dark",
  );
  useEffect(
    () =>
      onAppearanceChange(() =>
        setDark(document.documentElement.dataset.colorMode === "dark"),
      ),
    [],
  );
  return dark;
}
