import type { FileTreeNode } from "./fileTree";

export type FileTreeRow = {
  key: string;
  depth: number;
  parent: string | null;
  position: number;
  siblingCount: number;
} & (
  | { kind: "node"; node: FileTreeNode }
  | { kind: "more" | "loading"; path: string }
);

/** 只扁平化已展开节点，分页与等待行保留所属目录层级。 */
export function flattenFileTree(
  nodes: FileTreeNode[],
  isOpen: (path: string) => boolean,
  hasMore: (path: string) => boolean,
  loading: (path: string) => boolean,
): FileTreeRow[] {
  const rows: FileTreeRow[] = [];
  /** 目录下的分页操作放在其已加载子项之后。 */
  function append(
    children: FileTreeNode[],
    parent: string | null,
    depth: number,
  ): void {
    const path = parent ?? "";
    const siblingCount =
      children.length + (loading(path) || hasMore(path) ? 1 : 0);
    for (const [index, node] of children.entries()) {
      rows.push({
        kind: "node",
        node,
        key: `${node.kind}:${node.path}`,
        depth,
        parent,
        position: index + 1,
        siblingCount,
      });
      if (node.kind === "directory" && isOpen(node.path))
        append(node.children, node.path, depth + 1);
    }
    if (loading(path))
      rows.push({
        kind: "loading",
        path,
        key: `page:${path}`,
        depth,
        parent,
        position: siblingCount,
        siblingCount,
      });
    else if (hasMore(path))
      rows.push({
        kind: "more",
        path,
        key: `page:${path}`,
        depth,
        parent,
        position: siblingCount,
        siblingCount,
      });
  }
  append(nodes, null, 1);
  return rows;
}
