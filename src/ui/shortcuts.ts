/** 根据当前桌面平台提供与实际监听一致的快捷键提示。 */
export function shortcutLabel(key: string): string {
  return `${/Mac|iPhone|iPad/.test(navigator.platform) ? "⌘" : "Ctrl+"}${key.toUpperCase()}`;
}
