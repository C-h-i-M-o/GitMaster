import { useMemo, useState } from "react";
import { parseUnifiedDiff } from "../ui/diffPresentation";
import { foldDiffContext } from "../ui/diffFolding";

/** 差异解析和折叠状态随输入快照变化，切换文件不沿用旧展开状态。 */
export function useUnifiedDiff(content: string, untracked: boolean) {
  const diff = useMemo(
    () => parseUnifiedDiff(content, untracked),
    [content, untracked],
  );
  const defaultExpanded = useMemo(
    () =>
      new Set(
        foldDiffContext(diff.lines, new Set()).flatMap((row) =>
          row.kind === "fold" ? [row.id] : [],
        ),
      ),
    [diff],
  );
  const [state, setState] = useState<{
    source: typeof diff;
    expanded: ReadonlySet<string>;
  }>({ source: diff, expanded: defaultExpanded });
  const rows = useMemo(
    () =>
      foldDiffContext(
        diff.lines,
        state.source === diff ? state.expanded : defaultExpanded,
      ),
    [diff, state, defaultExpanded],
  );
  /** 用户主动收起片段，切换文件后仍默认展示完整上下文。 */
  function toggle(id: string): () => void {
    return () =>
      setState((current) => {
        const expanded = new Set(
          current.source === diff ? current.expanded : defaultExpanded,
        );
        if (expanded.has(id)) expanded.delete(id);
        else expanded.add(id);
        return { source: diff, expanded };
      });
  }
  return { diff, rows, toggle };
}
