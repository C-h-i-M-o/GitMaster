import test from "node:test";
import assert from "node:assert/strict";
import {
  toggleGroupSelection,
  groupSelection,
  reconcileChangeSelection,
} from "../../src/ui/virtualChanges.ts";
import type { FileChange } from "../../src/types/git.ts";
const files = Array.from({ length: 10000 }, (_, i): FileChange => ({
  changeId: String(i),
  path: `${i}.txt`,
  kind: "tracked",
  indexStatus: ".",
  worktreeStatus: "M",
  originalPath: null,
  binary: "text",
}));
test("分组全选覆盖虚拟视口外全部文件并保留其他组选择", () => {
  const selected = toggleGroupSelection(files, ["other"]);
  assert.equal(selected.length, 10001);
  assert.deepEqual(groupSelection(files, selected), {
    checked: true,
    mixed: false,
    selected: 10000,
    total: 10000,
  });
  assert.deepEqual(toggleGroupSelection(files, selected), ["other"]);
});

test("暂存后只清除失效方向，提交不再包含旧暂存选择", () => {
  const file = { ...files[0], indexStatus: "M", worktreeStatus: "." };
  assert.deepEqual(
    reconcileChangeSelection(
      { unstaged: [], untracked: [], staged: [file], conflicted: [] },
      { stage: [file.changeId], unstage: [] },
    ),
    { stage: [], unstage: [] },
  );
});

test("同一 MM 文件仍保留两个有效方向且不自动勾选新增文件", () => {
  const file = { ...files[0], indexStatus: "M", worktreeStatus: "M" };
  const selected = { stage: [file.changeId], unstage: [file.changeId] };
  assert.equal(
    reconcileChangeSelection(
      { unstaged: [file, files[1]], untracked: [], staged: [file], conflicted: [] },
      selected,
    ),
    selected,
  );
});

test("文件删除或变成冲突后移除选择，保留其他有效路径", () => {
  assert.deepEqual(
    reconcileChangeSelection(
      { unstaged: [files[1]], untracked: [], staged: [], conflicted: [files[0]] },
      { stage: ["0", "1", "deleted"], unstage: ["0"] },
    ),
    { stage: ["1"], unstage: [] },
  );
});
test("不可操作项不计入全选，部分选择显示半选", () => {
  const group = [...files.slice(0, 2), { ...files[2], kind: "submodule" }];
  assert.deepEqual(groupSelection(group, ["0"]), {
    checked: false,
    mixed: true,
    selected: 1,
    total: 2,
  });
  assert.deepEqual(toggleGroupSelection(group, ["0"]), ["0", "1"]);
});
