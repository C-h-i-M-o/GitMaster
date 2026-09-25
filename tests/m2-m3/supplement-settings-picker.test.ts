import assert from "node:assert/strict";
import test from "node:test";
import { defaultSettings } from "../../src/ui/settingsDraft.ts";
import { applyPickedSettingsPath } from "../../src/ui/settingsPathSelection.ts";

test("原生路径选择只更新指定配置草稿，保留参数及其他字段", () => {
  const draft = defaultSettings();
  draft.terminal.profiles[0]!.args = ["中文 参数"];
  const before = structuredClone(draft);
  const next = applyPickedSettingsPath(draft, draft, { kind: "shell", profileId: "system" }, "/tmp/中文 shell");
  assert.equal(next.terminal.profiles[0]!.executablePath, "/tmp/中文 shell");
  assert.deepEqual(next.terminal.profiles[0]!.args, ["中文 参数"]);
  assert.deepEqual(next.editor, draft.editor);
  assert.deepEqual(draft, before);
});

test("取消、草稿替换和已删除配置拒绝选择器迟到结果", () => {
  const draft = defaultSettings();
  const target = { kind: "shell" as const, profileId: "system" };
  assert.equal(applyPickedSettingsPath(draft, draft, target, null), draft);
  const newer = structuredClone(draft);
  assert.equal(applyPickedSettingsPath(newer, draft, target, "/tmp/shell"), newer);
  assert.equal(applyPickedSettingsPath(draft, draft, { kind: "shell", profileId: "gone" }, "/tmp/shell"), draft);
});

test("固定目录选择不修改项目目录策略，Git 路径仍为待应用草稿", () => {
  const draft = defaultSettings();
  const target = { kind: "directory" as const, profileId: "system" };
  assert.equal(applyPickedSettingsPath(draft, draft, target, "/tmp/目录"), draft);
  draft.terminal.profiles[0]!.cwd = { kind: "fixed", path: "/old" };
  const next = applyPickedSettingsPath(draft, draft, target, "/tmp/目录");
  assert.deepEqual(next.terminal.profiles[0]!.cwd, { kind: "fixed", path: "/tmp/目录" });
  assert.equal(applyPickedSettingsPath(draft, draft, { kind: "git" }, "/tmp/git").gitPath, "/tmp/git");
  assert.equal(draft.gitPath, null);
});
