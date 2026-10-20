/** 匹配 Windows 规范路径与扩展长度路径；其他平台保留大小写。 */
export function workspacePathKey(path: string): string {
  if (/^(?:\\\\\?\\)?[a-z]:[\\/]/i.test(path) || path.startsWith("\\\\"))
    return path
      .replace(/^\\\\\?\\UNC\\/i, "\\\\")
      .replace(/^\\\\\?\\/, "")
      .replaceAll("/", "\\")
      .replace(/\\+$/, "")
      .toLowerCase();
  return path.replace(/\/+$/, "");
}

/** 位置缓存最多保留 50 个文档，优先包含当前活动文档且不修改打开列表。 */
export function workspaceDocumentPaths(
  paths: readonly string[],
  activePath: string | null,
): string[] {
  const stored = paths.slice(0, 50);
  if (activePath && paths.includes(activePath) && !stored.includes(activePath))
    stored[stored.length - 1] = activePath;
  return stored;
}
