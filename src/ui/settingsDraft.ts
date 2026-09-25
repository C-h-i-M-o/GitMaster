import type {
  AppSettings,
  SettingsCategory,
  TerminalProfile,
} from "../types/settings";

/** 每次创建独立默认草稿，禁止共享终端参数数组。 */
export function defaultSettings(): AppSettings {
  return {
    version: 3,
    gitPath: null,
    logLevel: null,
    uiPreferences: { elasticity: 6, showLabels: true },
    terminal: {
      defaultProfileId: "system",
      profiles: [
        {
          profileId: "system",
          name: "系统默认",
          executablePath: null,
          args: [],
          cwd: { kind: "project" },
        },
      ],
      fontFamily: "Consolas, Menlo, monospace",
      fontSize: 14,
      cursorStyle: "block",
      cursorBlink: true,
      scrollbackLines: 5000,
    },
    editor: {
      fontFamily: "Consolas, Menlo, monospace",
      fontSize: 14,
      tabSize: 4,
      wordWrap: "off",
    },
    externalOpen: { defaultAppId: "fileManager" },
  };
}

/** 恢复只修改当前分类；调用者仍需点击应用才能保存。 */
export function resetSettingsCategory(
  draft: AppSettings,
  category: SettingsCategory,
): AppSettings {
  const defaults = defaultSettings();
  switch (category) {
    case "general":
      return { ...draft, gitPath: defaults.gitPath };
    case "appearance":
      return { ...draft, uiPreferences: defaults.uiPreferences };
    case "logging":
      return { ...draft, logLevel: defaults.logLevel };
    case "terminal":
      return restoreTerminalDefaults(draft, defaults);
    case "editor":
      return { ...draft, editor: defaults.editor };
    case "externalOpen":
      return { ...draft, externalOpen: defaults.externalOpen };
  }
}

/** 自动检测项必须使用默认参数和项目目录，不能覆盖用户改过的配置。 */
function systemProfile(draft: AppSettings): TerminalProfile | undefined {
  return draft.terminal.profiles.find(
    (profile) =>
      profile.executablePath === null &&
      profile.args.length === 0 &&
      profile.cwd.kind === "project",
  );
}

/** 满额且没有自动检测项时要求用户先腾出配置位置，不静默删除。 */
export function terminalResetBlocked(draft: AppSettings): boolean {
  return draft.terminal.profiles.length >= 32 && !systemProfile(draft);
}

/** 恢复显示默认与自动检测选择，完整保留已有配置及其稳定标识。 */
function restoreTerminalDefaults(
  draft: AppSettings,
  defaults: AppSettings,
): AppSettings {
  if (terminalResetBlocked(draft)) return draft;
  const existing = systemProfile(draft);
  const profiles = [...draft.terminal.profiles];
  let profileId = existing?.profileId ?? "system";
  if (!existing) {
    let suffix = 0;
    while (profiles.some((profile) => profile.profileId === profileId))
      profileId = `system-${++suffix}`;
    profiles.push({
      profileId,
      name: "系统默认",
      executablePath: null,
      args: [],
      cwd: { kind: "project" },
    });
  }
  return {
    ...draft,
    terminal: { ...defaults.terminal, profiles, defaultProfileId: profileId },
  };
}

/** 删除配置不影响运行会话，默认项必须由用户先明确选择替代配置。 */
export function removeTerminalProfile(
  draft: AppSettings,
  profileId: string,
): AppSettings {
  if (draft.terminal.defaultProfileId === profileId) return draft;
  const profiles = draft.terminal.profiles.filter(
    (profile) => profile.profileId !== profileId,
  );
  const first = profiles[0];
  if (!first || profiles.length === draft.terminal.profiles.length)
    return draft;
  return {
    ...draft,
    terminal: {
      ...draft.terminal,
      profiles,
    },
  };
}

/** 设置仅包含 JSON 值，按字段规范化顺序比较以识别真实草稿变化。 */
export function settingsDirty(saved: AppSettings, draft: AppSettings): boolean {
  return (
    JSON.stringify(canonicalSettings(saved)) !==
    JSON.stringify(canonicalSettings(draft))
  );
}

/** 显式构造稳定字段顺序，不依赖后端对象键的序列化顺序。 */
function canonicalSettings(value: AppSettings): unknown {
  return [
    value.version,
    value.gitPath,
    value.logLevel,
    value.uiPreferences.elasticity,
    value.uiPreferences.showLabels,
    value.terminal.defaultProfileId,
    value.terminal.fontFamily,
    value.terminal.fontSize,
    value.terminal.cursorStyle,
    value.terminal.cursorBlink,
    value.terminal.scrollbackLines,
    value.terminal.profiles.map((profile) => [
      profile.profileId,
      profile.name,
      profile.executablePath,
      profile.args,
      profile.cwd.kind,
      profile.cwd.kind === "fixed" ? profile.cwd.path : null,
    ]),
    value.editor.fontFamily,
    value.editor.fontSize,
    value.editor.tabSize,
    value.editor.wordWrap,
    value.externalOpen.defaultAppId,
  ];
}
