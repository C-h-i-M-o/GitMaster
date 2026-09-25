import {
  useCallback,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
} from "react";
import { defaultRangeExtractor, useVirtualizer } from "@tanstack/react-virtual";
import type { useFileTree } from "./useFileTree";
import { flattenFileTree, type FileTreeRow } from "../ui/virtualTree";

export type VirtualFileTreeState = Pick<
  ReturnType<typeof useFileTree>,
  "nodes" | "isOpen" | "toggle" | "hasMore" | "more" | "loading"
>;

/** 成熟虚拟器负责视口，单一树焦点保证行卸载时键盘仍可导航。 */
export function useVirtualFileTree(
  tree: VirtualFileTreeState,
  openFile: (id: string) => () => void,
) {
  const container = useRef<HTMLDivElement>(null);
  const prefix = useId();
  const [activeKey, setActiveKey] = useState<string | null>(null);
  const previousIndex = useRef(0);
  const rows = useMemo(
    () => flattenFileTree(tree.nodes, tree.isOpen, tree.hasMore, tree.loading),
    [tree.nodes, tree.isOpen, tree.hasMore, tree.loading],
  );
  const found = rows.findIndex((row) => row.key === activeKey);
  const activeIndex =
    found >= 0
      ? found
      : rows.length
        ? activeKey?.startsWith("page:")
          ? Math.min(previousIndex.current, rows.length - 1)
          : 0
        : -1;
  if (found >= 0) previousIndex.current = found;
  const getItemKey = useCallback(
    (index: number) => rows[index]?.key ?? index,
    [rows],
  );
  const virtual = useVirtualizer({
    count: rows.length,
    getScrollElement: () => container.current,
    estimateSize: () => 32,
    getItemKey,
    overscan: 6,
    rangeExtractor: useCallback(
      (range) => {
        const visible = defaultRangeExtractor(range);
        return activeIndex >= 0 && !visible.includes(activeIndex)
          ? [...visible, activeIndex].sort((a, b) => a - b)
          : visible;
      },
      [activeIndex],
    ),
  });
  useEffect(() => {
    if (found < 0 && activeKey !== null) {
      setActiveKey(rows[activeIndex]?.key ?? null);
      if (activeIndex >= 0)
        virtual.scrollToIndex(activeIndex, { align: "auto" });
    }
  }, [found, activeKey, activeIndex, rows, virtual]);
  /** 聚焦逻辑项时滚动到对应行，不强迫 DOM 接管焦点。 */
  function select(index: number): void {
    const row = rows[index];
    if (!row) return;
    setActiveKey(row.key);
    virtual.scrollToIndex(index, { align: "auto" });
  }
  /** 鼠标与 Enter 使用相同激活语义，加载行不产生动作。 */
  function activate(row: FileTreeRow): () => void {
    return () => {
      container.current?.focus({ preventScroll: true });
      setActiveKey(row.key);
      if (row.kind === "more") tree.more(row.path)();
      else if (row.kind === "node") {
        if (row.node.kind === "directory") tree.toggle(row.node.path)();
        else openFile(row.node.fileId)();
      }
    };
  }
  /** 支持方向键、首尾及激活，左右键遵循树展开和父项导航习惯。 */
  function keyDown(event: KeyboardEvent<HTMLDivElement>): void {
    const row = rows[activeIndex];
    if (
      !row ||
      ![
        "ArrowDown",
        "ArrowUp",
        "Home",
        "End",
        "ArrowRight",
        "ArrowLeft",
        "Enter",
        " ",
      ].includes(event.key)
    )
      return;
    event.preventDefault();
    if (event.key === "ArrowDown")
      select(Math.min(rows.length - 1, activeIndex + 1));
    else if (event.key === "ArrowUp") select(Math.max(0, activeIndex - 1));
    else if (event.key === "Home") select(0);
    else if (event.key === "End") select(rows.length - 1);
    else if (event.key === "Enter" || event.key === " ") activate(row)();
    else if (
      event.key === "ArrowRight" &&
      row.kind === "node" &&
      row.node.kind === "directory"
    ) {
      if (!tree.isOpen(row.node.path)) tree.toggle(row.node.path)();
      else if (rows[activeIndex + 1]?.parent === row.node.path)
        select(activeIndex + 1);
    } else if (event.key === "ArrowLeft") {
      if (
        row.kind === "node" &&
        row.node.kind === "directory" &&
        tree.isOpen(row.node.path)
      )
        tree.toggle(row.node.path)();
      else if (row.parent !== null)
        select(
          rows.findIndex(
            (candidate) =>
              candidate.kind === "node" &&
              candidate.node.kind === "directory" &&
              candidate.node.path === row.parent,
          ),
        );
    }
  }
  /** 活动项即使离开视口也保留 DOM，供读屏解析活动后代。 */
  function rowId(index: number): string {
    return `${prefix}-row-${index}`;
  }
  return {
    container,
    rows,
    items: virtual.getVirtualItems(),
    totalSize: virtual.getTotalSize(),
    activeIndex,
    activeId: activeIndex >= 0 ? rowId(activeIndex) : undefined,
    rowId,
    keyDown,
    activate,
  };
}
