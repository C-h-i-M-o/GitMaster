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
import type { DiffSide } from "../types/git";
import {
  flattenChanges,
  type ChangeGroups,
  type ChangeRow,
} from "../ui/virtualChanges";

export interface VirtualChangesProps {
  groups: ChangeGroups;
  selected: { stage: string[]; unstage: string[] };
  blocked: boolean;
  toggle(kind: "stage" | "unstage", id: string): () => void;
  inspect(id: string, side: DiffSide): () => void;
}

/** 变更列表只负责视口和导航，不改变工作台写入合同。 */
export function useVirtualChanges(props: VirtualChangesProps) {
  const container = useRef<HTMLDivElement>(null);
  const prefix = useId();
  const rows = useMemo(() => flattenChanges(props.groups), [props.groups]);
  const [activeKey, setActiveKey] = useState<string | null>(null);
  const found = rows.findIndex((row) => row.key === activeKey);
  const activeIndex =
    found >= 0 ? found : rows.findIndex((row) => row.kind === "file");
  const getItemKey = useCallback(
    (index: number) => rows[index]?.key ?? index,
    [rows],
  );
  const virtual = useVirtualizer({
    count: rows.length,
    getScrollElement: () => container.current,
    estimateSize: () => 36,
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
      if (activeIndex >= 0) virtual.scrollToIndex(activeIndex);
    }
  }, [found, activeKey, activeIndex, rows, virtual]);
  /** 单一焦点留在容器，虚拟行回收不会丢失键盘入口。 */
  function focus(row: ChangeRow): void {
    setActiveKey(row.key);
    container.current?.focus({ preventScroll: true });
  }
  /** 勾选只调用既有选择回调，活动任务及子模块不改变选择。 */
  function toggle(row: ChangeRow): () => void {
    return () => {
      if (row.kind !== "file") return;
      focus(row);
      if (!props.blocked && row.file.kind !== "submodule")
        props.toggle(row.action, row.file.changeId)();
    };
  }
  /** 查看不会隐式勾选，即使写入暂时禁用仍可阅读。 */
  function inspect(row: ChangeRow): () => void {
    return () => {
      if (row.kind !== "file") return;
      focus(row);
      props.inspect(row.file.changeId, row.side)();
    };
  }
  /** 在文件项间移动，跳过组头和空组说明。 */
  function move(start: number, direction: 1 | -1): void {
    for (
      let index = start;
      index >= 0 && index < rows.length;
      index += direction
    ) {
      const row = rows[index];
      if (row?.kind === "file") {
        focus(row);
        virtual.scrollToIndex(index, { align: "auto" });
        return;
      }
    }
  }
  /** Space 勾选、Enter 查看；原生子控件自身键盘事件不重复执行。 */
  function keyDown(event: KeyboardEvent<HTMLDivElement>): void {
    const row = rows[activeIndex];
    if (["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) {
      event.preventDefault();
      if (event.key === "Home") move(0, 1);
      else if (event.key === "End") move(rows.length - 1, -1);
      else
        move(
          activeIndex + (event.key === "ArrowDown" ? 1 : -1),
          event.key === "ArrowDown" ? 1 : -1,
        );
    } else if (
      event.target === event.currentTarget &&
      row &&
      (event.key === " " || event.key === "Enter")
    ) {
      event.preventDefault();
      if (event.key === " ") toggle(row)();
      else inspect(row)();
    }
  }
  /** 活动行与 DOM id 一一对应，提供读屏活动引用。 */
  function rowId(index: number): string {
    return `${prefix}-change-${index}`;
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
    toggle,
    inspect,
  };
}
