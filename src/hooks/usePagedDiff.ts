import {
  useEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
} from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import type { DiffScope, PagedDiff } from "../types/diff";
import { readDiffPage } from "../services/diff";
import { diffLayout } from "../ui/pagedDiffLayout";
import { createDiffPages } from "./diffPagesController";

export interface PagedDiffProps {
  scope: DiffScope;
  document: PagedDiff;
}
/** 固定行高虚拟差异视口，调用方以文档 ID 重建生命周期。 */
export function usePagedDiff({ scope, document }: PagedDiffProps) {
  const [controller] = useState(() =>
    createDiffPages(document.rowCount, (start, count, offset) =>
      readDiffPage(scope, document.documentId, start, count, offset),
    ),
  );
  const state = useSyncExternalStore(
    controller.subscribe,
    controller.getSnapshot,
  );
  const container = useRef<HTMLDivElement>(null);
  const [expanded, setExpanded] = useState<ReadonlySet<number>>(new Set());
  const positions = useMemo(
    () => diffLayout(document.rowCount, document.folds, expanded),
    [document, expanded],
  );
  const virtual = useVirtualizer({
    count: positions.length,
    getItemKey: (index) =>
      `${positions[index]?.kind}:${positions[index]?.source}`,
    getScrollElement: () => container.current,
    estimateSize: () => 28,
    overscan: 6,
  });
  const items = virtual.getVirtualItems();
  const start = items[0]?.index ?? 0;
  const end = (items.at(-1)?.index ?? -1) + 1;
  useEffect(() => {
    controller.activate();
    return () => controller.dispose();
  }, [controller]);
  useEffect(() => {
    if (end > start)
      void controller.ensureRows(
        positions
          .slice(Math.max(0, start - 10), end + 10)
          .flatMap((p) => (p.kind === "line" ? [p.source] : [])),
      );
  }, [controller, positions, start, end]);
  /** 长行切换仅替换当前片段，不积累整行正文。 */
  function segment(index: number, offset: number): () => void {
    return () => {
      void controller.segment(index, offset);
    };
  }
  /** 展开状态只影响位置映射，不修改统计、原始行号或后台文档。 */
  function toggle(source: number): () => void {
    return () =>
      setExpanded((current) => {
        const next = new Set(current);
        if (next.has(source)) next.delete(source);
        else next.add(source);
        return next;
      });
  }
  return {
    ...state,
    positions,
    toggle,
    container,
    items,
    totalSize: virtual.getTotalSize(),
    row: controller.row,
    segment,
  };
}
