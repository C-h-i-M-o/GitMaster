/** 在匹配的提交身份中循环定位；首次正向取第一项，反向取最后一项。 */
export function nextMatchedOid(
  oids: readonly string[],
  current: string | null,
  direction: 1 | -1,
): string | null {
  if (!oids.length) return null;
  const index = current ? oids.indexOf(current) : -1;
  return (
    oids[
      index < 0
        ? direction === 1
          ? 0
          : oids.length - 1
        : (index + direction + oids.length) % oids.length
    ] ?? null
  );
}
/** 只匹配提交元数据，不因输入触发后端读取或改变图拓扑。 */
export function matchesCommit(
  query: string,
  fields: readonly string[],
): boolean {
  const normalized = query.trim().toLocaleLowerCase();
  return (
    !normalized ||
    fields.some((value) => value.toLocaleLowerCase().includes(normalized))
  );
}
