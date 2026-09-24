import type {
  ClonePreview,
  RepositoryState,
  WritePreview,
} from "../types/git.ts";

/** 从执行前已确认的计划捕获显示摘要，后续切换分支不改变历史归属。 */
export function captureOperationTarget(
  preview: WritePreview | ClonePreview,
  repository: RepositoryState | null,
): string {
  if ("targetPath" in preview) return `下载到 ${preview.targetPath}`;
  const project =
    repository?.repositoryId === preview.repositoryId
      ? repository.rootPath
      : preview.repositoryId;
  const source =
    preview.head.kind === "detached"
      ? `分离头指针 ${preview.head.oid.slice(0, 7)}`
      : preview.head.name;
  const target = preview.target;
  const destination =
    target?.kind === "localBranch"
      ? target.name
      : target?.kind === "remote"
        ? target.refName
        : target?.kind === "merge"
          ? target.oid.slice(0, 7)
          : null;
  const files = preview.paths.length
    ? ` · ${preview.paths[0]}${preview.paths.length > 1 ? ` 等 ${preview.paths.length} 个文件` : ""}`
    : "";
  return `${project} · ${source}${destination ? ` → ${destination}` : ""}${files}`;
}
