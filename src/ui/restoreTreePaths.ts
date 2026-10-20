import type { ProjectTreePage } from "../types/git.ts";

/** 按相对路径重新签发目录页，缓存仅在同一个树会话内复用。 */
export async function restoreTreePaths(
  root: ProjectTreePage,
  paths: readonly string[],
  load: (id: string, offset: number) => Promise<ProjectTreePage>,
  current: () => boolean,
): Promise<Record<string, ProjectTreePage>> {
  const pages: Record<string, ProjectTreePage> = { [root.directoryId]: root };
  /** 所有等待边界重新检查树身份，禁止把旧能力合入新会话。 */
  function check(page?: ProjectTreePage): void {
    if (!current() || (page && page.treeId !== root.treeId))
      throw { code: "STALE_REQUEST" };
  }
  for (const path of [...new Set(paths)].slice(0, 200)) {
    if (
      !path ||
      path.split("/").some((part) => !part || part === ".." || part === ".")
    )
      continue;
    check();
    let parent = pages[root.directoryId]!;
    for (const part of path.split("/")) {
      let page = pages[parent.directoryId] ?? parent;
      let entry = page.entries.find(
        (item) => item.kind === "directory" && item.name === part,
      );
      while (!entry && page.nextOffset !== null) {
        const next = await load(page.directoryId, page.nextOffset);
        check(next);
        page = { ...next, entries: [...page.entries, ...next.entries] };
        pages[page.directoryId] = page;
        entry = page.entries.find(
          (item) => item.kind === "directory" && item.name === part,
        );
      }
      if (!entry) break;
      parent = pages[entry.id] ?? (await load(entry.id, 0));
      check(parent);
      pages[entry.id] = parent;
    }
  }
  return pages;
}
