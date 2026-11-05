import assert from "node:assert/strict";
import test from "node:test";
import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { createServer } from "vite";

test("会话配置按模型默认值继承，模型变更会清除参数快照", async () => {
  const server = await createServer({
    server: { middlewareMode: true },
    appType: "custom",
  });
  try {
    const helpers = await server.ssrLoadModule(
      "/src/ui/agentChatConfiguration.ts",
    );
    const defaults = {
      contextLimit: 65_536,
      maxOutputTokens: 8192,
      thinkingMode: "enabled",
      reasoningEffort: "high",
      thinkingBudgetTokens: null,
      temperature: null,
      topP: null,
    };
    const model = { id: "model-a", defaults };
    const draft = helpers.newChatConfiguration("model-a", "requestApproval");
    assert.equal(draft.parameters, null);
    assert.deepEqual(helpers.effectiveChatParameters(draft, model), defaults);
    const edited = helpers.updateChatParameter(
      draft,
      model,
      "maxOutputTokens",
      4096,
    );
    assert.equal(edited.parameters.maxOutputTokens, 4096);
    assert.deepEqual(edited.parameters, { ...defaults, maxOutputTokens: 4096 });
    const switched = helpers.chatConfigurationForModel(edited, "model-b");
    assert.equal(switched.parameters, null);
    const restored = helpers.restoreChatParameter(
      edited,
      model,
      "maxOutputTokens",
    );
    assert.equal(restored.parameters, null);
  } finally {
    await server.close();
  }
});

test("聊天面板展示会话模型与工作区，不使用全局模型选择", async () => {
  const server = await createServer({
    server: { middlewareMode: true },
    appType: "custom",
  });
  try {
    const { AgentPanel } = await server.ssrLoadModule(
      "/src/components/AgentPanel.tsx",
    );
    const model = {
      id: "chat-model",
      modelId: "vendor/chat-model",
      label: "聊天模型",
      protocol: "responses",
      enabled: true,
      revision: 1,
      capabilities: {
        contextWindow: null,
        maxOutputTokens: null,
        supportedEfforts: [],
        thinkingModes: [],
        thinkingBudget: false,
        toolSupport: null,
        samplingSupport: null,
        source: "unknown",
        verifiedAt: null,
        endpoint: null,
        parameterStyle: "generic",
      },
      defaults: {
        contextLimit: 32_768,
        maxOutputTokens: null,
        thinkingMode: null,
        reasoningEffort: null,
        thinkingBudgetTokens: null,
        temperature: null,
        topP: null,
      },
    };
    const session = {
      id: "chat-1",
      title: "会话标题",
      repositoryPath: "C:/repo",
      workspaceId: "folder-1",
      modelConfigId: model.id,
      permissionMode: "autoApprove",
      parameters: null,
      memory: { enabled: true, includeUser: false, excludedIds: [] },
      archived: false,
    };
    const controller = {
      settings: {
        enabled: true,
        view: {
          settings: {
            defaultPermissionMode: "requestApproval",
            activeModelConfigId: "global-model",
            models: [
              model,
              {
                ...model,
                id: "global-model",
                modelId: "global/model",
                label: "全局模型",
              },
            ],
          },
        },
        error: null,
        setupOpen: false,
        busy: false,
        add() {},
      },
      currentChatWorkspace: {
        id: "folder-1",
        kind: "folder",
        label: "项目目录",
        rootPath: "C:/repo",
        writePath: "C:/repo/output",
        generation: 1,
        gitAvailable: true,
      },
      selectedWorkspace: null,
      workspaceId: "folder-1",
      enabledModels: [model],
      recentModels: [],
      otherModels: [model],
      record: { session, events: [] },
      context: { open: false, show() {} },
      run: null,
      configuration: {
        modelConfigId: model.id,
        permissionMode: "autoApprove",
      },
      configurationBusy: false,
      configurationDirty: false,
      missingModel: false,
      mismatched: false,
      draft: "",
      disabled: false,
      busy: false,
      error: null,
      repositoryPath: "C:/repo",
      end: { current: null },
      messagesRoot: { current: null },
      scroll() {},
      send() {},
      changeDraft() {},
      keyDown() {},
      keyUp() {},
      composerBlur() {},
      changeModelConfig() {},
      changePermissionMode() {},
      saveChatConfiguration() {},
      ensureChatForMemory: async () => session,
      updateChatSession() {},
      cancel() {},
      chatParameterEditor() {
        return {
          model: {
            id: model.id,
            modelId: model.modelId,
            label: model.label,
            protocol: model.protocol,
            enabled: true,
            defaults: model.defaults,
            expectedRevision: 1,
          },
          modelCapabilities: model.capabilities,
          modelAvailability: {
            contextLimit: true,
            maxOutputTokens: true,
            thinkingMode: false,
            reasoningEffort: false,
            thinkingBudgetTokens: false,
            temperature: false,
            topP: false,
          },
          modelErrors: {},
          modelField: () => () => {},
          parameterField: () => () => {},
          restoreParameter: () => () => {},
        };
      },
    };
    const markup = renderToStaticMarkup(
      React.createElement(AgentPanel, { a: controller }),
    );
    assert.match(markup, /aria-label="当前聊天模型"/);
    assert.match(markup, /上下文与记忆/);
    assert.match(markup, /value="chat-model"/);
    assert.doesNotMatch(markup, /value="global-model"/);
    assert.match(markup, /C:\/repo\/output/);
    assert.match(markup, /value="autoApprove" selected=""/);
  } finally {
    await server.close();
  }
});

test("失联文件夹聊天仍可读取但工作区不可用于发送", async () => {
  const server = await createServer({
    server: { middlewareMode: true },
    appType: "custom",
  });
  try {
    const helpers = await server.ssrLoadModule(
      "/src/ui/agentChatConfiguration.ts",
    );
    const session = {
      workspaceId: "folder-1",
      repositoryPath: "C:/old/project",
    };
    const current = [
      { id: "default", kind: "default", rootPath: "C:/work" },
      { id: "folder-1", kind: "folder", rootPath: "D:/new/project" },
      { id: "folder-2", kind: "folder", rootPath: "E:/ignored" },
    ];
    assert.deepEqual(
      helpers.currentWorkspaces(current).map(({ id }) => id),
      ["default", "folder-1"],
    );
    assert.equal(helpers.sessionWorkspace(session, current), null);
  } finally {
    await server.close();
  }
});
