import assert from "node:assert/strict";
import test from "node:test";
import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { createServer } from "vite";

test("useAgent 在禁用与设置尚未加载时均可完成首屏服务端渲染", async () => {
  const server = await createServer({ server: { middlewareMode: true } });
  try {
    const { useAgent } = await server.ssrLoadModule("/src/hooks/useAgent.ts");
    /** 在真实React渲染中调用hook，并只输出初始展示所需的可观察状态。 */
    function Probe({ enabled }) {
      const agent = useAgent(enabled, null, async () => {});
      return React.createElement(
        "main",
        {
          "data-enabled": String(agent.settings.enabled),
          "data-settings-loaded": String(agent.settings.view !== null),
          "data-chat-loaded": String(agent.record !== null),
          "data-no-model": String(agent.enabledModels.length === 0),
          "data-disabled": String(agent.disabled),
        },
        "AI workspace",
      );
    }

    const disabledMarkup = renderToStaticMarkup(
      React.createElement(Probe, { enabled: false }),
    );
    const loadingMarkup = renderToStaticMarkup(
      React.createElement(Probe, { enabled: true }),
    );

    assert.match(disabledMarkup, /data-enabled="false"/);
    assert.match(disabledMarkup, /data-settings-loaded="false"/);
    assert.match(disabledMarkup, /data-chat-loaded="false"/);
    assert.match(disabledMarkup, /data-no-model="true"/);
    assert.match(loadingMarkup, /data-enabled="true"/);
    assert.match(loadingMarkup, /data-settings-loaded="false"/);
    assert.match(loadingMarkup, /data-chat-loaded="false"/);
    assert.match(loadingMarkup, /data-no-model="true"/);
    assert.match(loadingMarkup, /data-disabled="(?:true|false)"/);
    assert.match(loadingMarkup, /AI workspace/);
  } finally {
    await server.close();
  }
});
