/** 打开错误字段的所有折叠祖先；仅主动定位时滚动并聚焦。 */
export function revealSettingsField(
  root: HTMLElement | null,
  field: string,
  focus = false,
): void {
  const element = Array.from(
    root?.querySelectorAll<HTMLElement>("[data-settings-field]") ?? [],
  ).find((item) => item.dataset.settingsField === field);
  if (!element) return;
  let parent = element.parentElement;
  while (parent && parent !== root) {
    if (parent instanceof HTMLDetailsElement) parent.open = true;
    parent = parent.parentElement;
  }
  if (focus) {
    element.scrollIntoView({ block: "nearest" });
    element.focus();
  }
}

/** 日志级别与选项文案一致，不向普通用户展示英文枚举。 */
export function logLevelLabel(level: string | undefined): string {
  const labels: Record<string, string> = {
    error: "仅错误",
    warn: "警告及错误",
    info: "常规信息",
    debug: "调试信息",
    trace: "详细追踪",
  };
  return level ? (labels[level] ?? level) : "尚未读取";
}
