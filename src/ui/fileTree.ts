import type { ProjectFileList, ProjectFileKind } from "../types/git";

export type FileTreeNode =
  | { kind: "directory"; name: string; path: string; children: FileTreeNode[] }
  | {
      kind: "file";
      name: string;
      path: string;
      fileId: string;
      status: ProjectFileKind;
    };

/** 从后端签发清单建树；筛选只影响显示，不拼接文件访问路径。 */
export function buildFileTree(
  files: ProjectFileList["files"],
  query: string,
): FileTreeNode[] {
  const root: FileTreeNode[] = [];
  const directories = new Map<
    string,
    Extract<FileTreeNode, { kind: "directory" }>
  >();
  const search = query.trim().toLocaleLowerCase();
  for (const file of files) {
    if (!file.path.toLocaleLowerCase().includes(search)) continue;
    const parts = file.path.split("/");
    const name = parts.pop();
    if (name === undefined) continue;
    let children = root;
    let path = "";
    for (const part of parts) {
      path = path ? `${path}/${part}` : part;
      let directory = directories.get(path);
      if (!directory) {
        directory = { kind: "directory", name: part, path, children: [] };
        directories.set(path, directory);
        children.push(directory);
      }
      children = directory.children;
    }
    children.push({
      kind: "file",
      name,
      path: file.path,
      fileId: file.fileId,
      status: file.kind,
    });
  }
  sortNodes(root);
  return root;
}

/** 每层目录在前，使用固定语言排序并保留原路径大小写。 */
function sortNodes(nodes: FileTreeNode[]): void {
  nodes.sort((left, right) =>
    left.kind !== right.kind
      ? left.kind === "directory"
        ? -1
        : 1
      : left.name.localeCompare(right.name, "zh-CN", { numeric: true }),
  );
  for (const node of nodes)
    if (node.kind === "directory") sortNodes(node.children);
}
