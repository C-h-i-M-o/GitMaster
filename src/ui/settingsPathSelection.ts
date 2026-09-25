import type { AppSettings } from "../types/settings";

export type SettingsPathTarget =
  { kind: "git" } | { kind: "shell" | "directory"; profileId: string };

/** 选择器只更新原草稿；取消、草稿替换或目标消失都不能覆盖后续输入。 */
export function applyPickedSettingsPath(
  current: AppSettings,
  expected: AppSettings,
  target: SettingsPathTarget,
  path: string | null,
): AppSettings {
  if (current !== expected || path === null) return current;
  if (target.kind === "git") return { ...current, gitPath: path };
  const selected = current.terminal.profiles.find(
    (profile) => profile.profileId === target.profileId,
  );
  if (
    !selected ||
    (target.kind === "directory" && selected.cwd.kind !== "fixed")
  )
    return current;
  return {
    ...current,
    terminal: {
      ...current.terminal,
      profiles: current.terminal.profiles.map((profile) =>
        profile !== selected
          ? profile
          : target.kind === "shell"
            ? { ...profile, executablePath: path }
            : { ...profile, cwd: { kind: "fixed", path } },
      ),
    },
  };
}
