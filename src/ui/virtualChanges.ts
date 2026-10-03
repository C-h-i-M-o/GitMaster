import type { DiffSide, FileChange } from "../types/git";

export type ChangeGroups = Record<DiffSide, FileChange[]>;

/** 只计算当前分组可操作项，使用集合避免万文件列表反复线性查找。 */
export function groupSelection(
  files: readonly FileChange[],
  selected: readonly string[],
) {
  const selectable = files.filter(
    (file) => file.kind !== "submodule" && file.kind !== "conflicted",
  );
  const ids = new Set(selected);
  const count = selectable.filter((file) => ids.has(file.changeId)).length;
  return {
    checked: selectable.length > 0 && count === selectable.length,
    mixed: count > 0 && count < selectable.length,
    selected: count,
    total: selectable.length,
  };
}

/** 全选或取消本组全部可操作项，保留另一分组的选择。 */
export function toggleGroupSelection(
  files: readonly FileChange[],
  selected: readonly string[],
): string[] {
  const ids = new Set(selected);
  const remove = groupSelection(files, selected).checked;
  for (const file of files) {
    if (file.kind === "submodule" || file.kind === "conflicted") continue;
    if (remove) ids.delete(file.changeId);
    else ids.add(file.changeId);
  }
  return [...ids];
}
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
