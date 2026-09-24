import type { LogLevel, UiPreferences } from "./git";

export type SettingsCategory =
  "general" | "appearance" | "logging" | "terminal" | "editor" | "externalOpen";
export type TerminalDirectory =
  { kind: "project" } | { kind: "home" } | { kind: "fixed"; path: string };
export interface TerminalProfile {
  profileId: string;
  name: string;
  executablePath: string | null;
  args: string[];
  cwd: TerminalDirectory;
}
export interface TerminalPreferences {
  defaultProfileId: string;
  profiles: TerminalProfile[];
  fontFamily: string;
  fontSize: number;
  cursorStyle: "block" | "bar" | "underline";
  cursorBlink: boolean;
  scrollbackLines: number;
}
export interface EditorPreferences {
  fontFamily: string;
  fontSize: number;
  tabSize: 2 | 4 | 8;
  wordWrap: "off" | "on";
}
export interface AppSettings {
  version: 3;
  gitPath: string | null;
  logLevel: LogLevel | null;
  uiPreferences: UiPreferences;
  terminal: TerminalPreferences;
  editor: EditorPreferences;
  externalOpen: { defaultAppId: "fileManager" | "vsCode" | "terminal" };
}
export interface SettingsSnapshot {
  settings: AppSettings;
  revision: string;
}
export interface SettingsFieldError {
  field: string;
  message: string;
}
export interface SettingsError {
  code: string;
  fieldErrors: SettingsFieldError[];
}
