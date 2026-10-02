import {
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type SyntheticEvent,
} from "react";
import { branchHeads } from "../ui/branchHeadPresentation";
import type { HeadState, HistoryPage } from "../types/git";

/** 引用列表展开只影响当前节点，事件不触发画布拖动或提交选择。 */
export function useBranchHeads(
  refs: HistoryPage["tips"],
  head: HeadState | undefined,
  oid: string,
) {
  const [expanded, setExpanded] = useState(false);
  const container = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState({ width: 310, height: 42 });
  const heads = useMemo(() => branchHeads(refs, head, oid), [refs, head, oid]);
  useLayoutEffect(() => {
    const element = container.current;
    if (!element) return;
    /** 读取未乘画布缩放的实际标签尺寸，使 SVG 裁剪与命中区域一致。 */
    function measure(): void {
      if (!element) return;
      const next = { width: element.offsetWidth, height: element.offsetHeight };
      setSize((old) =>
        old.width === next.width && old.height === next.height ? old : next,
      );
    }
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    measure();
    return () => observer.disconnect();
  }, [heads, expanded]);
  /** 标签区域自行处理鼠标和键盘，阻止冒泡到节点。 */
  function stop(event: SyntheticEvent): void {
    event.stopPropagation();
  }
  /** 用户点击可展开全部引用，重复点击收起。 */
  function toggle(event: SyntheticEvent): void {
    event.stopPropagation();
    setExpanded((value) => !value);
  }
  return {
    heads,
    container,
    size,
    visible: expanded ? heads : heads.slice(0, 2),
    expanded,
    toggle,
    stop,
  };
}
