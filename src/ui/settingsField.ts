import type { SettingsCategory } from "../types/settings";

/** 将稳定字段路径转成中文分类，未知字段只展示原错误而不猜测控件。 */
export function settingsFieldLocation(
  field: string,
): { category: SettingsCategory; label: string } | null {
  const categories: Record<
    string,
    { category: SettingsCategory; label: string }
  > = {
    gitPath: { category: "general", label: "Git 环境" },
    uiPreferences: { category: "appearance", label: "外观" },
    logLevel: { category: "logging", label: "诊断日志" },
    terminal: { category: "terminal", label: "终端" },
    editor: { category: "editor", label: "文件编辑器" },
    externalOpen: { category: "externalOpen", label: "外部打开" },
  };
  const key = field.split(".")[0] ?? "";
  const base = Object.hasOwn(categories, key) ? categories[key] : undefined;
  if (!base) return null;
  const profile = /^terminal\.profiles\.(\d+)\./.exec(field);
  return {
    ...base,
    label: profile
      ? `${base.label} · 配置 ${Number(profile[1]) + 1}`
      : base.label,
  };
}
