export interface DiffLine {
  kind: "hunk" | "context" | "add" | "delete" | "note";
  oldLine: number | null;
  newLine: number | null;
  text: string;
}
export interface UnifiedDiff {
  lines: DiffLine[];
  additions: number;
  deletions: number;
}

/** 解析后端统一 patch；未跟踪内容是原文，不能按 patch 解释。 */
export function parseUnifiedDiff(
  content: string,
  untracked: boolean,
): UnifiedDiff {
  const result: UnifiedDiff = { lines: [], additions: 0, deletions: 0 };
  if (!content) return result;
  const lines = content.split("\n");
  if (lines.at(-1) === "") lines.pop();
  let oldLine = 0,
    newLine = 0,
    inside = false;
  for (const raw of lines) {
    const text = raw.endsWith("\r") ? raw.slice(0, -1) : raw;
    if (untracked) {
      result.lines.push({
        kind: "add",
        oldLine: null,
        newLine: ++newLine,
        text,
      });
      result.additions++;
      continue;
    }
    const hunk = /^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/.exec(text);
    if (hunk) {
      oldLine = Number(hunk[1]);
      newLine = Number(hunk[2]);
      inside = true;
      result.lines.push({ kind: "hunk", oldLine: null, newLine: null, text });
    } else if (inside && text.startsWith("+")) {
      result.lines.push({
        kind: "add",
        oldLine: null,
        newLine: newLine++,
        text: text.slice(1),
      });
      result.additions++;
    } else if (inside && text.startsWith("-")) {
      result.lines.push({
        kind: "delete",
        oldLine: oldLine++,
        newLine: null,
        text: text.slice(1),
      });
      result.deletions++;
    } else if (inside && text.startsWith(" ")) {
      result.lines.push({
        kind: "context",
        oldLine: oldLine++,
        newLine: newLine++,
        text: text.slice(1),
      });
    } else if (inside && text.startsWith("\\")) {
      result.lines.push({
        kind: "note",
        oldLine: null,
        newLine: null,
        text: "文件末尾没有换行符",
      });
    }
  }
  return result;
}
