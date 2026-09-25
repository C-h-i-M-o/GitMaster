import {
  useVirtualFileTree,
  type VirtualFileTreeState,
} from "../hooks/useVirtualFileTree";

/** 文件树只挂载视口行与活动行；行数据仍来自已授权的目录分页。 */
export function VirtualFileTree({
  tree,
  selectedPath,
  openFile,
}: {
  tree: VirtualFileTreeState;
  selectedPath: string;
  openFile: (id: string) => () => void;
}) {
  const view = useVirtualFileTree(tree, openFile);
  return (
    <div
      ref={view.container}
      className="virtual-file-tree"
      role="tree"
      aria-label="项目文件树"
      tabIndex={0}
      aria-activedescendant={view.activeId}
      onKeyDown={view.keyDown}
    >
      <div
        role="presentation"
        style={{ height: view.totalSize, position: "relative" }}
      >
        {view.items.map((item) => {
          const row = view.rows[item.index];
          if (!row) return null;
          const node = row.kind === "node" ? row.node : null;
          return (
            <div
              key={item.key}
              id={view.rowId(item.index)}
              role="treeitem"
              aria-level={row.depth}
              aria-posinset={row.position}
              aria-setsize={row.siblingCount}
              aria-expanded={
                node?.kind === "directory" ? tree.isOpen(node.path) : undefined
              }
              aria-selected={
                node?.kind === "file" && node.path === selectedPath
              }
              aria-disabled={row.kind === "loading" || undefined}
              className={`project-tree-row virtual-tree-row ${item.index === view.activeIndex ? "keyboard-active" : ""} ${node?.path === selectedPath ? "selected" : ""}`}
              title={node?.path}
              onClick={view.activate(row)}
              style={{
                position: "absolute",
                top: 0,
                left: 0,
                height: item.size,
                transform: `translateY(${item.start}px)`,
                paddingLeft: 6 + (row.depth - 1) * 14,
              }}
            >
              <span aria-hidden="true">
                {node?.kind === "directory"
                  ? tree.isOpen(node.path)
                    ? "▾"
                    : "▸"
                  : node
                    ? "◇"
                    : ""}
              </span>
              <span>
                {node?.name ?? (row.kind === "more" ? "加载更多" : "正在读取…")}
              </span>
              {node?.kind === "file" && node.status === "untracked" && (
                <small>新文件</small>
              )}
              {node?.kind === "file" && node.status === "ignored" && (
                <small>已忽略</small>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
