import type * as monaco from "monaco-editor/editor/editor.api.js";
import type { ITheme } from "@xterm/xterm";
import { activeAppearance } from "./appearanceRuntime";
import { appearancePalette } from "./themes";

/** 将当前 Codex 色值应用到 Monaco；语法角色使用该主题终端色。 */
export function monacoTheme(): monaco.editor.IStandaloneThemeData {
  const resolved = activeAppearance();
  const p = appearancePalette(resolved);
  const c = resolved.colors;
  return {
    base: resolved.mode === "dark" ? "vs-dark" : "vs",
    inherit: true,
    rules: [
      { token: "comment", foreground: p["--muted"]!.slice(1) },
      { token: "string", foreground: p["--success"]!.slice(1) },
      { token: "number", foreground: p["--warning"]!.slice(1) },
      { token: "keyword", foreground: p["--purple"]!.slice(1) },
      { token: "type", foreground: p["--info"]!.slice(1) },
    ],
    colors: {
      ...c,
      "editor.background": p["--paper"]!,
      "editor.foreground": p["--ink"]!,
      "editorLineNumber.foreground": p["--muted"]!,
      "editorLineNumber.activeForeground": p["--green"]!,
      "editorGutter.background": p["--paper"]!,
      "editorWidget.background": p["--surface"]!,
      "editorWidget.border": p["--line"]!,
      "editor.selectionBackground": p["--selection"]!,
    },
  };
}

/** 终端更新配色选项，不影响 PTY、已输出内容或用户输入。 */
export function terminalTheme(): ITheme {
  const resolved = activeAppearance();
  const c = resolved.colors,
    p = appearancePalette(resolved);
  return {
    background: p["--terminal-background"],
    foreground: p["--terminal-foreground"],
    cursor: p["--terminal-cursor"],
    cursorAccent: p["--paper"],
    selectionBackground: p["--terminal-selection"],
    black: c["terminal.ansiBlack"] ?? p["--muted"],
    red: c["terminal.ansiRed"] ?? p["--danger"],
    green: c["terminal.ansiGreen"] ?? p["--success"],
    yellow: c["terminal.ansiYellow"] ?? p["--warning"],
    blue: c["terminal.ansiBlue"] ?? p["--info"],
    magenta: c["terminal.ansiMagenta"] ?? p["--purple"],
    cyan: c["terminal.ansiCyan"] ?? p["--green"],
    white: c["terminal.ansiWhite"] ?? p["--ink"],
    brightBlack: c["terminal.ansiBrightBlack"] ?? p["--muted"],
    brightRed: c["terminal.ansiBrightRed"] ?? p["--danger"],
    brightGreen: c["terminal.ansiBrightGreen"] ?? p["--success"],
    brightYellow: c["terminal.ansiBrightYellow"] ?? p["--warning"],
    brightBlue: c["terminal.ansiBrightBlue"] ?? p["--info"],
    brightMagenta: c["terminal.ansiBrightMagenta"] ?? p["--purple"],
    brightCyan: c["terminal.ansiBrightCyan"] ?? p["--green"],
    brightWhite: c["terminal.ansiBrightWhite"] ?? p["--ink"],
  };
}
