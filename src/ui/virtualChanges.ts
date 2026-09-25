import type { DiffSide, FileChange } from "../types/git";

export type ChangeGroups = Record<DiffSide, FileChange[]>;
export type ChangeRow = {
  key: string;
  side: DiffSide;
  action: "stage" | "unstage";
} & (
  | { kind: "header"; title: string; count: number }
  | { kind: "empty"; title: string }
  | { kind: "file"; file: FileChange }
);
const GROUPS: ReadonlyArray<{
  side: DiffSide;
  title: string;
  action: "stage" | "unstage";
}> = [
  { side: "unstaged", title: "未暂存", action: "stage" },
  { side: "untracked", title: "未跟踪", action: "stage" },
  { side: "staged", title: "已暂存", action: "unstage" },
];

/** 分组扁平化不合并比较侧，同一 MM 文件保留两个独立操作位置。 */
export function flattenChanges(groups: ChangeGroups): ChangeRow[] {
  return GROUPS.flatMap((group): ChangeRow[] => [
    {
      ...group,
      kind: "header",
      key: `header:${group.side}`,
      count: groups[group.side].length,
    },
    ...(groups[group.side].length
      ? groups[group.side].map((file): ChangeRow => ({
          ...group,
          kind: "file",
          key: `${group.side}:${file.changeId}`,
          file,
        }))
      : [{ ...group, kind: "empty" as const, key: `empty:${group.side}` }]),
  ]);
}
