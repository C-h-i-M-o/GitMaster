import assert from "node:assert/strict";
import test from "node:test";
import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { createServer } from "vite";

test("AI 侧栏渲染聊天选择与原生可键盘访问的聊天菜单", async () => {
  const server = await createServer({ server: { middlewareMode: true } });
  try {
    const { AgentSidebar } = await server.ssrLoadModule(
      "/src/components/AgentSidebar.tsx",
    );
    const chat = {
      id: "chat-1",
      title: "修复登录流程",
      archived: false,
      repositoryPath: "C:/work/project",
      updatedAt: 10,
    };
    const controller = {
      chats: [chat],
      record: { session: chat },
      title: chat.title,
      titleValidation: { count: chat.title.length, error: null },
      renameTarget: null,
      deletePending: null,
      busy: false,
      run: null,
      archived: false,
      unavailable: [],
      create() {},
      toggleArchived() {},
      select() {
        return () => {};
      },
      beginRename() {
        return () => {};
      },
      archive() {
        return () => {};
      },
      askDelete() {
        return () => {};
      },
      changeTitle() {},
      rename() {},
      cancelRename() {},
      keepChat() {},
      deleteChat() {},
    };
    const markup = renderToStaticMarkup(
      React.createElement(AgentSidebar, {
        a: controller,
        repositoryPath: chat.repositoryPath,
      }),
    );

    assert.match(markup, /aria-label="AI 聊天列表"/);
    assert.match(markup, /修复登录流程/);
    assert.match(markup, /<details class="agent-chat-menu">/);
    assert.match(markup, /aria-label="管理聊天：修复登录流程"/);
    assert.match(markup, /归档聊天/);
    assert.match(markup, /删除聊天/);
  } finally {
    await server.close();
  }
});
