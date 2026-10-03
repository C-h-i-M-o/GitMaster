import test from "node:test";
import assert from "node:assert/strict";
import { SettingsAutosaveController } from "../src/services/settingsAutosaveController.ts";
import { defaultSettings } from "../src/ui/settingsDraft.ts";
import type { SettingsPatch, SettingsSnapshot } from "../src/types/settings.ts";
import { mergeSettingsPatch } from "../src/ui/settingsPatch.ts";
const snap = (revision = "r0"): SettingsSnapshot => ({
  revision,
  settings: defaultSettings(),
});
const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

test("快速编辑最终值落入 saved", async () => {
  const calls: SettingsPatch[] = [];
  let saved = snap();
  const c = new SettingsAutosaveController(saved, async (_r, p) => {
    calls.push(p);
    saved = {
      revision: `r${calls.length}`,
      settings: mergeSettingsPatch(saved.settings, p),
    };
    return saved;
  });
  c.edit((s) => ({ ...s, editor: { ...s.editor, fontSize: 18 } }));
  c.edit((s) => ({ ...s, editor: { ...s.editor, fontSize: 20 } }));
  await c.flush();
  assert.equal(calls.length, 1);
  assert.equal(c.getSnapshot().saved.settings.editor.fontSize, 20);
});
test("inFlight 编辑保留 draft，flush 等到最新值", async () => {
  let release: (() => void) | undefined;
  const calls: SettingsPatch[] = [];
  const c = new SettingsAutosaveController(snap(), async (_r, p) => {
    calls.push(p);
    if (calls.length === 1)
      await new Promise<void>((r) => {
        release = r;
      });
    return snap(`r${calls.length}`);
  });
  c.edit((s) => ({ ...s, editor: { ...s.editor, fontSize: 18 } }), true);
  await wait(10);
  c.edit((s) => ({ ...s, editor: { ...s.editor, saveMode: "auto" } }), true);
  const done = c.flush();
  assert.equal(c.getSnapshot().draft.editor.saveMode, "auto");
  release?.();
  await done;
  assert.equal(calls.length, 2);
});
test("fontSize 失败后 saveMode 仍可独立保存", async () => {
  const c = new SettingsAutosaveController(snap(), async (_r, p) => {
    if (p.kind === "editor" && "fontSize" in p)
      throw {
        code: "INVALID_INPUT",
        fieldErrors: [{ field: "editor.fontSize", message: "非法" }],
      };
    return snap("r1");
  });
  c.edit((s) => ({ ...s, editor: { ...s.editor, fontSize: 99 } }), true);
  await wait(10);
  c.edit((s) => ({ ...s, editor: { ...s.editor, saveMode: "auto" } }), true);
  await c.flush();
  assert.equal(c.getSnapshot().draft.editor.fontSize, 99);
  assert.equal(c.getSnapshot().error?.code, "INVALID_INPUT");
});
test("失败保留 error/draft 且不产生未处理异常", async () => {
  const c = new SettingsAutosaveController(snap(), async () => {
    throw { code: "SETTINGS_IO", fieldErrors: [] };
  });
  c.edit((s) => ({ ...s, editor: { ...s.editor, fontSize: 99 } }), true);
  await c.flush();
  assert.equal(c.getSnapshot().error?.code, "SETTINGS_IO");
  assert.equal(c.getSnapshot().draft.editor.fontSize, 99);
});
test("dispose 取消待保存计时器", async () => {
  let count = 0;
  const c = new SettingsAutosaveController(snap(), async () => {
    count++;
    return snap("r1");
  });
  c.edit((s) => ({ ...s, editor: { ...s.editor, fontSize: 18 } }));
  c.dispose();
  await wait(450);
  assert.equal(count, 0);
});
test("IME 组合期间不保存，compositionEnd 后保存", async () => {
  let count = 0;
  const c = new SettingsAutosaveController(snap(), async () => {
    count++;
    return snap("r1");
  });
  c.setComposing(true);
  c.edit((s) => ({ ...s, editor: { ...s.editor, fontFamily: "中文" } }));
  await wait(450);
  assert.equal(count, 0);
  c.setComposing(false);
  await c.flush();
  assert.equal(count, 1);
});
test("STALE 后重读外部 fontFamily，不覆盖本地 fontSize", async () => {
  let latest = {
    ...snap("r1"),
    settings: {
      ...defaultSettings(),
      editor: { ...defaultSettings().editor, fontFamily: "外部" },
    },
  };
  let first = true;
  const c = new SettingsAutosaveController(
    snap(),
    async (rev, p) => {
      if (first) {
        first = false;
        throw { code: "STALE_SETTINGS" };
      }
      return {
        revision: "r2",
        settings: {
          ...latest.settings,
          editor: {
            ...latest.settings.editor,
            fontSize:
              (p as Extract<SettingsPatch, { kind: "editor" }>).fontSize ??
              latest.settings.editor.fontSize,
          },
        },
      };
    },
    () => {},
    async () => latest,
  );
  c.edit((s) => ({ ...s, editor: { ...s.editor, fontSize: 20 } }), true);
  await c.flush();
  assert.equal(c.getSnapshot().saved.settings.editor.fontFamily, "外部");
  assert.equal(c.getSnapshot().saved.settings.editor.fontSize, 20);
  assert.equal(c.getSnapshot().draft.editor.fontSize, 20);
});
test("外部同字段冲突保留输入，显式 flush 重试可保存", async () => {
  let first = true;
  const c = new SettingsAutosaveController(
    snap(),
    async (_r, p) => {
      if (first) {
        first = false;
        throw { code: "STALE_SETTINGS" };
      }
      return {
        revision: "r3",
        settings: {
          ...defaultSettings(),
          editor: { ...defaultSettings().editor, ...p },
        },
      };
    },
    () => {},
    async () => ({
      ...snap("r2"),
      settings: {
        ...defaultSettings(),
        editor: { ...defaultSettings().editor, fontSize: 18 },
      },
    }),
  );
  c.edit((s) => ({ ...s, editor: { ...s.editor, fontSize: 20 } }), true);
  await c.flush(false);
  assert.equal(c.getSnapshot().error?.code, "STALE_SETTINGS");
  await c.flush(true);
  assert.equal(c.getSnapshot().error, null);
});
