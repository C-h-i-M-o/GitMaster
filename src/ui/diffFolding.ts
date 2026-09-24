import type { DiffLine } from "./diffPresentation";

export type DiffRow =
  | { kind: "line"; index: number; line: DiffLine }
  | { kind: "fold"; id: string; count: number; expanded: boolean };

/** 仅折叠已加载的连续上下文；保留两端三行，不跨 hunk 或注释。 */
export function foldDiffContext(
  lines: readonly DiffLine[],
  expanded: ReadonlySet<string>,
): DiffRow[] {
  const rows: DiffRow[] = [];
  let index = 0;
  while (index < lines.length) {
    const start = index;
    while (index < lines.length && lines[index]?.kind === "context") index++;
    const count = index - start;
    if (count > 8) {
      const id = `${start}:${index}`;
      for (let cursor = start; cursor < start + 3; cursor++)
        appendLine(rows, lines, cursor);
      rows.push({
        kind: "fold",
        id,
        count: count - 6,
        expanded: expanded.has(id),
      });
      if (expanded.has(id))
        for (let cursor = start + 3; cursor < index - 3; cursor++)
          appendLine(rows, lines, cursor);
      for (let cursor = index - 3; cursor < index; cursor++)
        appendLine(rows, lines, cursor);
    } else {
      for (let cursor = start; cursor < index; cursor++)
        appendLine(rows, lines, cursor);
    }
    if (index < lines.length) appendLine(rows, lines, index++);
  }
  return rows;
}

/** 原样引用解析行，折叠不重新编号、不改变文本或统计。 */
function appendLine(
  rows: DiffRow[],
  lines: readonly DiffLine[],
  index: number,
): void {
  const line = lines[index];
  if (line) rows.push({ kind: "line", index, line });
}
