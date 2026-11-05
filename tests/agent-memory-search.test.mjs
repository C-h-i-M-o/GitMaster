import assert from "node:assert/strict";
import test from "node:test";
import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { createServer } from "vite";

test("聊天记忆默认不跨工作区，只有显式允许才包含用户级偏好", async () => {
  const server = await createServer({
    server: { middlewareMode: true, hmr: false },
    appType: "custom",
  });
  try {
    const helpers = await server.ssrLoadModule("/src/ui/agentMemory.ts");
    const defaults = helpers.defaultChatMemoryConfig();
    assert.deepEqual(defaults, {
      enabled: true,
      includeUser: false,
      excludedIds: [],
    });
    const session = {
      memory: { enabled: true, includeUser: false, excludedIds: [] },
    };
    assert.equal(helpers.chatMemoryMatchesSession(defaults, session), true);
    assert.deepEqual(helpers.parseMemoryTags("偏好, 项目,偏好,,").sort(), [
      "偏好",
      "项目",
    ]);
    assert.equal(
      helpers.chatMemoryMatchesSession(
        { ...defaults, includeUser: true },
        session,
      ),
      false,
    );
    assert.deepEqual(
      helpers.memorySuggestionRequest(
        "chat-1",
        "workspace-1",
        7,
        "保留英文变量名",
        ["代码风格"],
        "workspace",
      ),
      {
        id: null,
        expectedRevision: null,
        scope: "workspace",
        workspaceId: "workspace-1",
        text: "保留英文变量名",
        tags: ["代码风格"],
        source: { chatId: "chat-1", sequence: 7 },
        enabled: true,
      },
    );
    assert.equal(
      helpers.memorySuggestionRequest(
        "chat-1",
        "workspace-1",
        7,
        "偏好",
        [],
        "user",
      ).workspaceId,
      null,
    );
  } finally {
    await server.close();
  }
});

test("记忆面板明确区分聊天记忆和手工条目管理", async () => {
  const server = await createServer({
    server: { middlewareMode: true, hmr: false },
  });
  try {
    const { AgentMemoryPanel } = await server.ssrLoadModule(
      "/src/components/AgentMemoryPanel.tsx",
    );
    const markup = renderToStaticMarkup(
      React.createElement(AgentMemoryPanel, {
        enabled: true,
        workspaceId: "default",
        chat: null,
        running: false,
        ensureChat: async () => {
          throw new Error("SSR 不会触发保存动作");
        },
        onChatUpdated() {},
      }),
    );

    assert.match(markup, /上下文与记忆/);
    assert.match(markup, /本聊天记忆/);
    assert.match(markup, /明确允许加入用户级跨工作区偏好/);
    assert.match(markup, /手工记忆/);
    assert.match(markup, /记忆仅由你手工保存/);
  } finally {
    await server.close();
  }
});

test("归档聊天的记忆选项只读，但工作区记忆仍可管理", async () => {
  const server = await createServer({
    server: { middlewareMode: true, hmr: false },
  });
  try {
    const { AgentMemoryPanel } = await server.ssrLoadModule(
      "/src/components/AgentMemoryPanel.tsx",
    );
    const markup = renderToStaticMarkup(
      React.createElement(AgentMemoryPanel, {
        enabled: true,
        workspaceId: "workspace-1",
        chat: {
          id: "archived-chat",
          archived: true,
          memory: { enabled: true, includeUser: false, excludedIds: [] },
        },
        running: false,
        ensureChat: async () => { throw new Error("归档聊天不会创建新会话"); },
        onChatUpdated() {},
      }),
    );
    const config = markup.match(
      /<section class="agent-memory-chat-config">([\s\S]*?)<\/section>/,
    )?.[1];

    assert.ok(config);
    assert.match(config, /disabled=""/);
    assert.doesNotMatch(config, /保存本聊天记忆设置/);
    assert.match(markup, /归档聊天的记忆设置只读/);
    assert.match(markup, /新增记忆/);
  } finally {
    await server.close();
  }
});
