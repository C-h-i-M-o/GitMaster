import assert from "node:assert/strict";
import test from "node:test";
import { defaultSettings, resetSettingsCategory } from "../../src/ui/settingsDraft.ts";

test("恢复终端默认保留用户配置与其他分类草稿", () => {
  const draft = defaultSettings();
  draft.terminal.profiles.push({
    profileId: "custom",
    name: "自定义",
    executablePath: "/tmp/中文 shell",
    args: ["--login", "空格 参数"],
    cwd: { kind: "fixed", path: "/tmp/项目" },
  });
  draft.terminal.defaultProfileId = "custom";
  draft.terminal.fontSize = 28;
  draft.editor.fontSize = 22;
  const before = structuredClone(draft);
  const restored = resetSettingsCategory(draft, "terminal");
  assert.deepEqual(restored.terminal.profiles, before.terminal.profiles);
  assert.equal(restored.terminal.defaultProfileId, "system");
  assert.equal(restored.terminal.fontSize, 14);
  assert.equal(restored.editor.fontSize, 22);
  assert.deepEqual(draft, before);
  assert.deepEqual(resetSettingsCategory(restored, "terminal"), restored);
});

test("系统标识已用于自定义程序时不覆盖，新增自动检测项后重复恢复不增殖", () => {
  const draft = defaultSettings();
  draft.terminal.profiles[0]!.executablePath = "/tmp/custom-shell";
  draft.terminal.profiles.push({
    profileId: "system-1",
    name: "另一个配置",
    executablePath: null,
    args: ["--custom"],
    cwd: { kind: "home" },
  });
  const before = structuredClone(draft.terminal.profiles);
  const restored = resetSettingsCategory(draft, "terminal");
  assert.deepEqual(restored.terminal.profiles.slice(0, 2), before);
  const selected = restored.terminal.profiles.find(
    (p) => p.profileId === restored.terminal.defaultProfileId,
  );
  assert.ok(selected);
  assert.equal(selected.executablePath, null);
  assert.deepEqual(selected.args, []);
  assert.deepEqual(selected.cwd, { kind: "project" });
  assert.equal(new Set(restored.terminal.profiles.map((p) => p.profileId)).size, 3);
  assert.deepEqual(resetSettingsCategory(restored, "terminal"), restored);
});

test("配置已满且没有自动检测项时保持草稿，不能删除配置腾位置", () => {
  const draft = defaultSettings();
  draft.terminal.profiles = Array.from({ length: 32 }, (_, index) => ({
    profileId: `custom-${index}`,
    name: `配置 ${index}`,
    executablePath: "/tmp/shell",
    args: [],
    cwd: { kind: "project" as const },
  }));
  draft.terminal.defaultProfileId = "custom-0";
  draft.terminal.fontSize = 25;
  assert.deepEqual(resetSettingsCategory(draft, "terminal"), draft);
});
