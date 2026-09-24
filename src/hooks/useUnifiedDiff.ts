import { useMemo, useState } from "react";
import { parseUnifiedDiff } from "../ui/diffPresentation";
import { foldDiffContext } from "../ui/diffFolding";

/** 差异解析和折叠状态随输入快照变化，切换文件不沿用旧展开状态。 */
export function useUnifiedDiff(content: string, untracked: boolean) {
  const diff = useMemo(
    () => parseUnifiedDiff(content, untracked),
    [content, untracked],
  );
  const [state, setState] = useState<{
    source: typeof diff;
    expanded: ReadonlySet<string>;
  }>({ source: diff, expanded: new Set() });
  const rows = useMemo(
    () =>
      foldDiffContext(
        diff.lines,
        state.source === diff ? state.expanded : new Set(),
      ),
    [diff, state],
  );
  /** 切换当前片段，变化后重新进入默认折叠状态。 */
  function toggle(id: string): () => void {
    return () =>
      setState((current) => {
        const expanded = new Set(
          current.source === diff ? current.expanded : [],
        );
        if (expanded.has(id)) expanded.delete(id);
        else expanded.add(id);
        return { source: diff, expanded };
      });
  }
  return { diff, rows, toggle };
}
