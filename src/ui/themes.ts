import type { CSSProperties } from "react";
import type { UiPreferences } from "../types/git.ts";
import type {
  ColorMode,
  ThemeMode,
  LightTheme,
  DarkTheme,
} from "../types/appearance.ts";
import { themeColors } from "./themeColors.ts";

export interface ThemeDefinition {
  id: DarkTheme;
  name: string;
  light: string | null;
  dark: string;
}
/** 与截图一致的主题顺序，浅色仅包含来源存在的变体。 */
export const themeCatalog: readonly ThemeDefinition[] = [
  {
    id: "absolutely",
    name: "Absolutely",
    light: "absolutely-light",
    dark: "absolutely-dark",
  },
  {
    id: "ayu",
    name: "Ayu",
    light: null,
    dark: "ayu-dark",
  },
  {
    id: "catppuccin",
    name: "Catppuccin",
    light: "catppuccin-latte",
    dark: "catppuccin-mocha",
  },
  {
    id: "dracula",
    name: "Dracula",
    light: null,
    dark: "dracula",
  },
  {
    id: "everforest",
    name: "Everforest",
    light: "everforest-light",
    dark: "everforest-dark",
  },
  {
    id: "github",
    name: "GitHub",
    light: "github-light",
    dark: "github-dark",
  },
  {
    id: "gruvbox",
    name: "Gruvbox",
    light: "gruvbox-light",
    dark: "gruvbox-dark",
  },
  {
    id: "linear",
    name: "Linear",
    light: "linear-light",
    dark: "linear-dark",
  },
  {
    id: "lobster",
    name: "Lobster",
    light: null,
    dark: "lobster-dark",
  },
  {
    id: "material",
    name: "Material",
    light: null,
    dark: "material-dark",
  },
  {
    id: "matrix",
    name: "Matrix",
    light: null,
    dark: "matrix-dark",
  },
  {
    id: "monokai",
    name: "Monokai",
    light: null,
    dark: "monokai",
  },
  {
    id: "night-owl",
    name: "Night Owl",
    light: null,
    dark: "night-owl",
  },
  {
    id: "nord",
    name: "Nord",
    light: null,
    dark: "nord",
  },
  {
    id: "notion",
    name: "Notion",
    light: "notion-light",
    dark: "notion-dark",
  },
  {
    id: "og",
    name: "OG",
    light: null,
    dark: "og-dark",
  },
  {
    id: "one",
    name: "One",
    light: "one-light",
    dark: "one-dark",
  },
  {
    id: "oscurange",
    name: "Oscurange",
    light: null,
    dark: "oscurange",
  },
  {
    id: "raycast",
    name: "Raycast",
    light: "raycast-light",
    dark: "raycast-dark",
  },
  {
    id: "rose-pine",
    name: "Rose Pine",
    light: "rose-pine-light",
    dark: "rose-pine-dark",
  },
  {
    id: "sentry",
    name: "Sentry",
    light: null,
    dark: "sentry-dark",
  },
  {
    id: "solarized",
    name: "Solarized",
    light: "solarized-light",
    dark: "solarized-dark",
  },
  {
    id: "temple",
    name: "Temple",
    light: null,
    dark: "temple-dark",
  },
  {
    id: "tokyo-night",
    name: "Tokyo Night",
    light: null,
    dark: "tokyo-night",
  },
  {
    id: "vercel",
    name: "Vercel",
    light: "vercel-light",
    dark: "vercel-dark",
  },
  {
    id: "vscode-plus",
    name: "VS Code Plus",
    light: "vscode-light",
    dark: "vscode-dark",
  },
  {
    id: "xcode",
    name: "Xcode",
    light: "xcode-light",
    dark: "xcode-dark",
  },
];

export interface ResolvedAppearance {
  mode: ThemeMode;
  theme: ThemeDefinition;
  colors: Readonly<Record<string, string>>;
}

/** 根据实际系统状态解析模式，不修改持久化偏好。 */
export function resolveAppearance(
  preferences: Pick<UiPreferences, "colorMode" | "lightTheme" | "darkTheme">,
  systemDark: boolean,
): ResolvedAppearance {
  const mode =
    preferences.colorMode === "system"
      ? systemDark
        ? "dark"
        : "light"
      : preferences.colorMode;
  const id = mode === "light" ? preferences.lightTheme : preferences.darkTheme;
  const theme =
    themeCatalog.find((item) => item.id === id && item[mode]) ??
    themeCatalog.find((item) => item.id === "github")!;
  return { mode, theme, colors: themeColors[theme[mode]!]! };
}

/** 控件输入经过目录校验，拒绝无对应变体的主题。 */
export function isLightTheme(value: string): value is LightTheme {
  return themeCatalog.some(
    (theme) => theme.id === value && theme.light !== null,
  );
}
/** 深色目录与受限主题类型保持一致。 */
export function isDarkTheme(value: string): value is DarkTheme {
  return themeCatalog.some((theme) => theme.id === value);
}
/** 模式只能来自三个固定选项。 */
export function isColorMode(value: string): value is ColorMode {
  return value === "system" || value === "light" || value === "dark";
}

/** 将带透明度的来源颜色合成到实际背景，供画布和对比度计算使用。 */
export function opaqueColor(color: string, background: string): string {
  if (!/^#[0-9a-f]{8}$/i.test(color)) return color;
  const alpha = parseInt(color.slice(7), 16) / 255;
  return (
    "#" +
    [1, 3, 5]
      .map((index) =>
        Math.round(
          parseInt(color.slice(index, index + 2), 16) * alpha +
            parseInt(background.slice(index, index + 2), 16) * (1 - alpha),
        )
          .toString(16)
          .padStart(2, "0"),
      )
      .join("")
  );
}
/** 计算 sRGB 亮度以保持主题按钮和弱化文字可读。 */
function luminance(color: string): number {
  const values = [1, 3, 5].map((index) => {
    const value = parseInt(color.slice(index, index + 2), 16) / 255;
    return value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
  });
  return values[0]! * 0.2126 + values[1]! * 0.7152 + values[2]! * 0.0722;
}
/** 保留原主题颜色；原色不足以阅读时使用该主题正文色。 */
function readable(color: string, background: string, fallback: string): string {
  const a = luminance(opaqueColor(color, background)),
    b = luminance(background);
  return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05) >= 4.5
    ? color
    : fallback;
}

/** 从 Codex Workbench 色值映射 GitMaster 的语义角色。 */
export function appearancePalette(
  resolved: ResolvedAppearance,
): Record<string, string> {
  const c = resolved.colors;
  const bg = c["editor.background"]!;
  const text = c["editor.foreground"]!;
  const accent =
    c["button.background"] ??
    c["focusBorder"] ??
    c["textLink.foreground"] ??
    text;
  const dark = resolved.mode === "dark";
  return {
    "--paper": bg,
    "--canvas": c["sideBar.background"] ?? bg,
    "--surface": c["panel.background"] ?? bg,
    "--ink": text,
    "--muted": readable(c.descriptionForeground ?? text, bg, text),
    "--accent": accent,
    "--green": readable(c["textLink.foreground"] ?? accent, bg, text),
    "--on-accent":
      luminance(opaqueColor(accent, bg)) > 0.179 ? "#000000" : "#ffffff",
    "--line":
      c["panel.border"] ??
      c["sideBar.border"] ??
      (dark ? "#ffffff30" : "#00000025"),
    "--selection":
      c["editor.selectionBackground"] ?? (dark ? "#ffffff25" : "#00000015"),
    "--success": readable(
      c["gitDecoration.addedResourceForeground"] ??
        c["gitDecoration.untrackedResourceForeground"] ??
        (dark ? "#65c99a" : "#24703a"),
      bg,
      text,
    ),
    "--danger": readable(
      c["gitDecoration.deletedResourceForeground"] ??
        c.errorForeground ??
        (dark ? "#f28b82" : "#b42318"),
      bg,
      text,
    ),
    "--warning": readable(
      c["terminal.ansiYellow"] ?? (dark ? "#f8b86f" : "#805900"),
      bg,
      text,
    ),
    "--info": readable(
      c["terminal.ansiBlue"] ?? (dark ? "#8dc8f4" : "#0969da"),
      bg,
      text,
    ),
    "--purple": readable(
      c["terminal.ansiMagenta"] ?? (dark ? "#b4a1e5" : "#8250df"),
      bg,
      text,
    ),
    "--terminal-background": bg,
    "--terminal-foreground": text,
    "--terminal-cursor": c["editorCursor.foreground"] ?? text,
    "--terminal-selection": opaqueColor(
      c["editor.selectionBackground"] ?? (dark ? "#ffffff30" : "#00000020"),
      bg,
    ),
  };
}

/** 主题选择器展示该模式的真实背景、正文和强调色。 */
export function themePreview(mode: ThemeMode, id: DarkTheme): CSSProperties {
  const definition = themeCatalog.find((theme) => theme.id === id)!;
  const colors = themeColors[definition[mode] ?? definition.dark]!;
  return {
    backgroundColor: colors["editor.background"],
    color:
      colors["textLink.foreground"] ??
      colors["button.background"] ??
      colors["editor.foreground"],
  };
}
