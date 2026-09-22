/** 按换行符拆分冲突文本，保留 BOM 与末尾空行。 */
export function splitConflictLines(value: string | null): string[] {
  return value === null ? [] : value.split(/\r\n|\n|\r/);
}
/** 计算三栏对齐所需的最大行数，不向源文本写入填充行。 */
export function alignConflictLines(
  local: string | null,
  incoming: string | null,
  result: string,
): {
  localLines: string[];
  incomingLines: string[];
  resultLines: string[];
  maxLines: number;
} {
  const localLines = splitConflictLines(local);
  const incomingLines = splitConflictLines(incoming);
  const resultLines = splitConflictLines(result);
  return {
    localLines,
    incomingLines,
    resultLines,
    maxLines: Math.max(
      1,
      localLines.length,
      incomingLines.length,
      resultLines.length,
    ),
  };
}
