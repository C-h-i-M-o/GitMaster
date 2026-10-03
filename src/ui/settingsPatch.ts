import type { AppSettings, SettingsPatch } from "../types/settings.ts";

/** 将单字段补丁合并进快照，终端配置关联单元保持整体一致。 */
export function mergeSettingsPatch(
  settings: AppSettings,
  patch: SettingsPatch,
): AppSettings {
  const { kind, ...values } = patch;
  switch (kind) {
    case "git":
      return {
        ...settings,
        gitPath: (patch as Extract<SettingsPatch, { kind: "git" }>).gitPath,
      };
    case "logging":
      return {
        ...settings,
        logLevel: (patch as Extract<SettingsPatch, { kind: "logging" }>)
          .logLevel,
      };
    case "appearance":
      return {
        ...settings,
        uiPreferences: { ...settings.uiPreferences, ...values },
      };
    case "terminalProfiles":
    case "terminalDisplay":
      return { ...settings, terminal: { ...settings.terminal, ...values } };
    case "editor":
      return { ...settings, editor: { ...settings.editor, ...values } };
    case "externalOpen":
      return {
        ...settings,
        externalOpen: (
          patch as Extract<SettingsPatch, { kind: "externalOpen" }>
        ).externalOpen,
      };
  }
}

/** 只提交实际改变的字段，非法输入不阻止同一分类的其他有效字段。 */
export function changedSettingsPatches(
  old: AppSettings,
  next: AppSettings,
): SettingsPatch[] {
  const patches: SettingsPatch[] = [];
  if (old.gitPath !== next.gitPath)
    patches.push({ kind: "git", gitPath: next.gitPath });
  if (old.logLevel !== next.logLevel)
    patches.push({ kind: "logging", logLevel: next.logLevel });
  for (const field of ["elasticity", "showLabels"] as const) {
    if (old.uiPreferences[field] !== next.uiPreferences[field])
      patches.push({ kind: "appearance", [field]: next.uiPreferences[field] });
  }
  if (
    JSON.stringify(old.terminal.profiles) !==
      JSON.stringify(next.terminal.profiles) ||
    old.terminal.defaultProfileId !== next.terminal.defaultProfileId
  ) {
    patches.push({
      kind: "terminalProfiles",
      profiles: next.terminal.profiles,
      defaultProfileId: next.terminal.defaultProfileId,
    });
  }
  for (const field of [
    "fontFamily",
    "fontSize",
    "cursorStyle",
    "cursorBlink",
    "scrollbackLines",
  ] as const) {
    if (old.terminal[field] !== next.terminal[field])
      patches.push({ kind: "terminalDisplay", [field]: next.terminal[field] });
  }
  for (const field of [
    "fontFamily",
    "fontSize",
    "tabSize",
    "wordWrap",
    "saveMode",
  ] as const) {
    if (old.editor[field] !== next.editor[field])
      patches.push({ kind: "editor", [field]: next.editor[field] });
  }
  if (old.externalOpen.defaultAppId !== next.externalOpen.defaultAppId)
    patches.push({ kind: "externalOpen", externalOpen: next.externalOpen });
  return patches;
}

/** 普通字段独立排队，关联终端配置共享一个队列槽。 */
export function settingsPatchKey(patch: SettingsPatch): string {
  return patch.kind === "terminalProfiles"
    ? patch.kind
    : `${patch.kind}:${Object.keys(patch)
        .filter((key) => key !== "kind")
        .sort()
        .join(",")}`;
}

/** 补丁队列键映射为设置表单中的稳定字段定位。 */
export function settingsPatchField(patch: SettingsPatch): string {
  if (patch.kind === "git") return "gitPath";
  if (patch.kind === "logging") return "logLevel";
  if (patch.kind === "terminalProfiles") return "terminal.profiles";
  if (patch.kind === "externalOpen") return "externalOpen.defaultAppId";
  const field = Object.keys(patch).find((key) => key !== "kind") ?? "";
  return `${patch.kind === "appearance" ? "uiPreferences" : patch.kind === "terminalDisplay" ? "terminal" : "editor"}.${field}`;
}
