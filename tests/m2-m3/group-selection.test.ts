import test from "node:test";
import assert from "node:assert/strict";
import {
  toggleGroupSelection,
  groupSelection,
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
