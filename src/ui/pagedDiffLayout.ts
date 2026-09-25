import type { DiffFold } from "../types/diff";

export type DiffPosition =
  | { kind: "line"; source: number }
  | { kind: "fold"; source: number; count: number; expanded: boolean };
/** 仅映射行位置，不加载正文；折叠按钮保留两侧各三行上下文。 */
export function diffLayout(
  rowCount: number,
  folds: readonly DiffFold[],
  expanded: ReadonlySet<number>,
): DiffPosition[] {
  const result: DiffPosition[] = [];
  let cursor = 0;
  for (const fold of folds) {
    const middle = fold.startRow + 3;
    while (cursor < middle) result.push({ kind: "line", source: cursor++ });
    const open = expanded.has(fold.startRow);
    result.push({
      kind: "fold",
      source: fold.startRow,
      count: fold.count - 6,
      expanded: open,
    });
    if (open)
      while (cursor < fold.startRow + fold.count - 3)
        result.push({ kind: "line", source: cursor++ });
    else cursor = fold.startRow + fold.count - 3;
  }
  while (cursor < rowCount) result.push({ kind: "line", source: cursor++ });
  return result;
}
