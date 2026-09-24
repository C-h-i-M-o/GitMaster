import { useMemo, useState, type SyntheticEvent } from "react";
import { branchHeads } from "../ui/branchHeadPresentation";
import type { HeadState, HistoryPage } from "../types/git";

/** 引用列表展开只影响当前节点，事件不触发画布拖动或提交选择。 */
export function useBranchHeads(
  refs: HistoryPage["tips"],
  head: HeadState | undefined,
  oid: string,
) {
  const [expanded, setExpanded] = useState(false);
  const heads = useMemo(() => branchHeads(refs, head, oid), [refs, head, oid]);
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
    visible: expanded ? heads : heads.slice(0, 2),
    expanded,
    toggle,
    stop,
  };
}
