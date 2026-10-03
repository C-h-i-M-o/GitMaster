import test from "node:test";
import assert from "node:assert/strict";
import { nextMatchedOid, matchesCommit } from "../src/ui/commitSearch.ts";
test("提交搜索首次、循环、空结果与分页追加身份稳定", () => {
  assert.equal(nextMatchedOid(["a", "b", "c"], null, 1), "a");
  assert.equal(nextMatchedOid(["a", "b", "c"], null, -1), "c");
  assert.equal(nextMatchedOid(["a", "b", "c"], "c", 1), "a");
  assert.equal(nextMatchedOid(["a", "b", "c"], "a", -1), "c");
  assert.equal(nextMatchedOid([], null, 1), null);
  assert.equal(nextMatchedOid(["new", "a", "b", "c"], "b", 1), "c");
});
test("匹配去除首尾空白并忽略大小写，支持作者与引用", () => {
  assert.equal(
    matchesCommit("  FEATURE  ", ["a1b2", "fix bug", "作者", "feature/main"]),
    true,
  );
  assert.equal(matchesCommit("作者", ["a1b2", "fix bug", "作者"]), true);
  assert.equal(matchesCommit("absent", ["a1b2", "fix bug", "作者"]), false);
});
