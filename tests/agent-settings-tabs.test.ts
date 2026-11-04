import assert from "node:assert/strict";
import test from "node:test";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { createServer } from "vite";
import type { Workbench } from "../src/hooks/useWorkbench.ts";
import { defaultSettings, resetSettingsCategory } from "../src/ui/settingsDraft.ts";
import { settingsFieldLocation } from "../src/ui/settingsField.ts";

/** 为服务端渲染提供目标分类所需的最小工作台状态。 */
function workbench(category: "general" | "agent"): Workbench {
  const settings = {
    enabled: false,
    busy: false,
    view: null,
    error: null,
    notice: null,
    add() {},
    select() {},
    permission() {},
    deletePending: null,
    usage: null,
    usageError: null,
    usageBusy: false,
    refreshUsage() {},
  };
  return {
    appSettings: {
      draft: defaultSettings(),
      editable: false,
      activity: "idle",
      error: null,
      flushOnBlur() {},
      compositionStart() {},
      compositionEnd() {},
    },
    settingsCategory: category,
    selectSettings: () => () => {},
    agent: { settings },
    preview: false,
    logSettings: {
      settings: null,
      busy: false,
      openDirectory() {},
      error: null,
    },
    applySettings() {},
  } as unknown as Workbench;
}

test("AI 与通用分区各自呈现对应设置内容", async () => {
  const server = await createServer({
    configFile: "vite.config.ts",
    server: { middlewareMode: true },
    appType: "custom",
  });
  try {
    const { SettingsPanel } = await server.ssrLoadModule(
      "/src/components/SettingsPanel.tsx",
    );
    const agentMarkup = renderToStaticMarkup(
      createElement(SettingsPanel, { w: workbench("agent") }),
    );
    const generalMarkup = renderToStaticMarkup(
      createElement(SettingsPanel, { w: workbench("general") }),
    );

    assert.match(agentMarkup, /class="agent-settings"/);
    assert.doesNotMatch(agentMarkup, /class="agent-storage"/);
    assert.doesNotMatch(agentMarkup, /data-settings-field="editor\.fontSize"/);
    assert.match(generalMarkup, /class="agent-storage"/);
    assert.doesNotMatch(generalMarkup, /class="agent-settings"/);
    assert.match(generalMarkup, /data-settings-field="editor\.fontSize"/);
  } finally {
    await server.close();
  }
});

test("Agent 分类恢复保留通用设置，复杂字段定位仍归属通用分类", () => {
  const draft = defaultSettings();
  draft.editor.fontSize = 19;
  draft.gitPath = "C:/tools/git.exe";

  assert.equal(resetSettingsCategory(draft, "agent"), draft);
  assert.deepEqual(settingsFieldLocation("terminal.profiles.2.executablePath"), {
    category: "general",
    label: "终端 · 配置 3",
  });
  assert.deepEqual(settingsFieldLocation("uiPreferences.darkTheme"), {
    category: "appearance",
    label: "外观",
  });
  assert.deepEqual(settingsFieldLocation("gitPath"), {
    category: "git",
    label: "Git 环境",
  });
  assert.equal(draft.editor.fontSize, 19);
  assert.equal(draft.gitPath, "C:/tools/git.exe");
});
