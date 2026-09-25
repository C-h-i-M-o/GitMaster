import type { ProjectFileList, ProjectTreePage } from "../types/git";

/** 将已加载页面的文件能力合并，目录不冒充可编辑文件。 */
export function mergeTreeFiles(
  current: ProjectFileList | null,
  page: ProjectTreePage,
): ProjectFileList {
  const files = new Map(
    (current?.snapshotId === page.snapshotId &&
    current.repositoryId === page.repositoryId
      ? current.files
      : []
    ).map((file) => [file.path, file]),
  );
  for (const entry of page.entries) {
    if (entry.kind === "file")
      files.set(entry.path, {
        fileId: entry.id,
        path: entry.path,
        kind: entry.status,
      });
  }
  return {
    repositoryId: page.repositoryId,
    snapshotId: page.snapshotId,
    files: [...files.values()],
  };
}
