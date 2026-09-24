import { useMemo, useState, type ChangeEvent } from "react";
import type { ProjectFileList } from "../types/git";
import { buildFileTree } from "../ui/fileTree";

const EMPTY_FILES: ProjectFileList["files"] = [];

/** 文件筛选和目录展开只维护界面状态，不发起磁盘写操作。 */
export function useFileTree(list: ProjectFileList | null) {
  const [query, setQuery] = useState("");
  const [closed, setClosed] = useState<ReadonlySet<string>>(new Set());
  const nodes = useMemo(
    () => buildFileTree(list?.files ?? EMPTY_FILES, query),
    [list, query],
  );
  /** 搜索时自动展示匹配祖先，不改变原折叠选择。 */
  function isOpen(path: string): boolean {
    return query.trim() !== "" || !closed.has(path);
  }
  /** 点击目录切换子列表，搜索状态维持展开以便定位结果。 */
  function toggle(path: string): () => void {
    return () => {
      if (query.trim()) return;
      setClosed((current) => {
        const next = new Set(current);
        if (next.has(path)) next.delete(path);
        else next.add(path);
        return next;
      });
    };
  }
  /** 按完整路径筛选，无须改变后端签发文件列表。 */
  function filter(event: ChangeEvent<HTMLInputElement>): void {
    setQuery(event.target.value);
  }
  return { nodes, query, filter, isOpen, toggle };
}
