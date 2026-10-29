import type { LogLevel, UiPreferences } from "./git";

export type SettingsCategory =
  | "general"
  | "appearance"
  | "logging"
  | "terminal"
  | "editor"
  | "externalOpen"
  | "cache";
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
  saveMode: "manual" | "auto";
}
export interface AppSettings {
  diskCacheEnabled: boolean;
  version: 4;
  gitPath: string | null;
  logLevel: LogLevel | null;
  uiPreferences: UiPreferences;
  terminal: TerminalPreferences;
  editor: EditorPreferences;
  externalOpen: { defaultAppId: "fileManager" | "vsCode" | "terminal" };
}
export type SettingsPatch =
  | { kind: "cache"; diskCacheEnabled: boolean }
  | { kind: "git"; gitPath: string | null }
  | { kind: "logging"; logLevel: LogLevel | null }
  | ({ kind: "appearance" } & Partial<UiPreferences>)
  | {
      kind: "terminalProfiles";
      profiles: TerminalProfile[];
      defaultProfileId: string;
    }
  | ({ kind: "terminalDisplay" } & Partial<
      Pick<
        TerminalPreferences,
        | "fontFamily"
        | "fontSize"
        | "cursorStyle"
        | "cursorBlink"
        | "scrollbackLines"
      >
    >)
  | ({ kind: "editor" } & Partial<EditorPreferences>)
  | { kind: "externalOpen"; externalOpen: AppSettings["externalOpen"] };
export interface SettingsSnapshot {
  settings: AppSettings;
  revision: string;
}
export interface CacheUsage {
  diskBytes: number;
  memoryBytes: number;
  limitBytes: number;
  enabled: boolean;
}
export interface SettingsFieldError {
  field: string;
  message: string;
}
export interface SettingsError {
  code: string;
  fieldErrors: SettingsFieldError[];
}
