/** 用户可选择的外观模式；系统模式在运行期间自动跟随。 */
export type ColorMode = "system" | "light" | "dark";
export type ThemeMode = Exclude<ColorMode, "system">;
/** 仅包含 Codex 实际提供的浅色主题。 */
export type LightTheme =
  | "absolutely"
  | "catppuccin"
  | "everforest"
  | "github"
  | "gruvbox"
  | "linear"
  | "notion"
  | "one"
  | "raycast"
  | "rose-pine"
  | "solarized"
  | "vercel"
  | "vscode-plus"
  | "xcode";
/** 图示全部 27 个深色主题。 */
export type DarkTheme =
  | "absolutely"
  | "ayu"
  | "catppuccin"
  | "dracula"
  | "everforest"
  | "github"
  | "gruvbox"
  | "linear"
  | "lobster"
  | "material"
  | "matrix"
  | "monokai"
  | "night-owl"
  | "nord"
  | "notion"
  | "og"
  | "one"
  | "oscurange"
  | "raycast"
  | "rose-pine"
  | "sentry"
  | "solarized"
  | "temple"
  | "tokyo-night"
  | "vercel"
  | "vscode-plus"
  | "xcode";
