import type { HeadState, HistoryPage } from "../types/git.ts";

export interface BranchHead {
  id: string;
  name: string;
  kind: "local" | "remote" | "detached";
  current: boolean;
}

/** 当前本地分支优先且不改变原引用数组，分离 HEAD 独立展示。 */
export function branchHeads(
  refs: HistoryPage["tips"],
  head: HeadState | undefined,
  oid: string,
): BranchHead[] {
  const result = refs.map((ref) => ({
    id: ref.refId,
    name: ref.name,
    kind: ref.kind,
    current:
      ref.kind === "local" &&
      head?.kind === "branch" &&
      head.name === ref.name &&
      head.oid === oid,
  }));
  result.sort(
    (a, b) =>
      Number(b.current) - Number(a.current) ||
      a.kind.localeCompare(b.kind) ||
      a.name.localeCompare(b.name) ||
      a.id.localeCompare(b.id),
  );
  if (head?.kind === "detached" && head.oid === oid)
    return [
      {
        id: "detached-head",
        name: "分离头指针",
        kind: "detached",
        current: true,
      },
      ...result,
    ];
  return result;
}
