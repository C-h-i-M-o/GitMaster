import assert from "node:assert/strict";
import test from "node:test";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { createServer } from "vite";

test("模型设置按连接展示同一供应商的多个模型", async () => {
  const server = await createServer({
    configFile: "vite.config.ts",
    server: { middlewareMode: true },
    appType: "custom",
  });
  try {
    const { AgentSettingsPanel } = await server.ssrLoadModule(
      "/src/components/AgentSettingsPanel.tsx",
    );
    const capabilities = {
      contextWindow: 1_000_000,
      maxOutputTokens: 64_000,
      supportedEfforts: ["low", "high"],
      thinkingModes: ["enabled", "disabled"],
      thinkingBudget: false,
      toolSupport: true,
      samplingSupport: true,
      source: "preset",
      verifiedAt: "2026-11-05",
      endpoint: "https://api.deepseek.com/chat/completions",
      parameterStyle: "deepseek",
    };
    const view = {
      settings: {
        version: 2,
        profiles: [],
        activeProfileId: null,
        defaultPermissionMode: "requestApproval",
        providers: [
          {
            id: "provider-a",
            name: "DeepSeek 连接",
            baseUrl: "https://api.deepseek.com",
            credentialId: "credential-a",
            presetId: "deepseek",
            revision: 1,
          },
        ],
        models: [
          {
            id: "model-a",
            providerId: "provider-a",
            modelId: "deepseek-flash",
            label: "DeepSeek Flash",
            protocol: "chatCompletions",
            enabled: true,
            revision: 1,
            capabilities,
            defaults: {
              contextLimit: null,
              maxOutputTokens: 8192,
              thinkingMode: null,
              reasoningEffort: null,
              thinkingBudgetTokens: null,
              temperature: null,
              topP: null,
            },
          },
          {
            id: "model-b",
            providerId: "provider-a",
            modelId: "deepseek-v4-pro",
            label: "DeepSeek Pro",
            protocol: "chatCompletions",
            enabled: true,
            revision: 1,
            capabilities: { ...capabilities, toolSupport: false },
            defaults: {
              contextLimit: null,
              maxOutputTokens: 8192,
              thinkingMode: null,
              reasoningEffort: null,
              thinkingBudgetTokens: null,
              temperature: null,
              topP: null,
            },
          },
        ],
        activeModelConfigId: "model-a",
        permissionReviewRequired: false,
      },
      hasApiKeys: {},
      providerHasApiKeys: { "provider-a": true },
      presets: [],
      warnings: [],
    };
    const settings = {
      enabled: true,
      busy: false,
      view,
      error: null,
      notice: null,
      add() {},
      select() {},
      permission() {},
      deletePending: null,
      keepProfile() {},
      confirmDelete() {},
      refreshUsage() {},
      usageBusy: false,
      configuration: {
        editProvider: () => () => {},
        editModel: () => () => {},
        addModel: () => () => {},
        requestDeleteProvider: () => () => {},
        toggleSavedModel: () => () => {},
        deletePending: null,
        keepProvider() {},
        confirmDeleteProvider() {},
      },
    };
    const markup = renderToStaticMarkup(
      createElement(AgentSettingsPanel, { s: settings }),
    );

    assert.match(markup, /class="agent-provider-card"/);
    assert.match(markup, /DeepSeek 连接/);
    assert.match(markup, /DeepSeek Flash/);
    assert.match(markup, /DeepSeek Pro/);
    assert.match(markup, /明确不支持 Agent 工具/);
  } finally {
    await server.close();
  }
});

test("未知模型只设置本地预算，能力声明按协议端点匹配", async () => {
  const server = await createServer({
    configFile: "vite.config.ts",
    server: { middlewareMode: true },
    appType: "custom",
  });
  try {
    const helpers = await server.ssrLoadModule(
      "/src/ui/agentModelConfiguration.ts",
    );
    const unknown = helpers.modelDraftForManual(
      "vendor-model",
      "responses",
      null,
    );
    assert.equal(unknown.defaults.contextLimit, 32_768);
    assert.equal(unknown.defaults.maxOutputTokens, null);
    const capabilities = {
      contextWindow: null,
      maxOutputTokens: null,
      supportedEfforts: [],
      thinkingModes: [],
      thinkingBudget: false,
      toolSupport: null,
      samplingSupport: null,
      source: "preset",
      verifiedAt: null,
      endpoint: "https://api.openai.com/v1/responses",
      parameterStyle: "openai",
    };
    assert.equal(
      helpers.capabilitiesMatchEndpoint(
        capabilities,
        "https://api.openai.com/v1",
        "responses",
      ),
      true,
    );
    assert.equal(
      helpers.capabilitiesMatchEndpoint(
        capabilities,
        "https://api.openai.com/v1",
        "chatCompletions",
      ),
      false,
    );
  } finally {
    await server.close();
  }
});

test("思考和采样参数按服务端能力矩阵禁用且保留冲突值", async () => {
  const server = await createServer({
    configFile: "vite.config.ts",
    server: { middlewareMode: true },
    appType: "custom",
  });
  try {
    const helpers = await server.ssrLoadModule(
      "/src/ui/agentModelConfiguration.ts",
    );
    const model = {
      id: null,
      modelId: "gpt-5.4",
      label: "GPT-5.4",
      protocol: "responses",
      enabled: true,
      expectedRevision: null,
      defaults: {
        contextLimit: null,
        maxOutputTokens: 8192,
        thinkingMode: "enabled",
        reasoningEffort: "high",
        thinkingBudgetTokens: null,
        temperature: 0.7,
        topP: null,
      },
    };
    const capabilities = {
      contextWindow: 1_000_000,
      maxOutputTokens: 64_000,
      supportedEfforts: ["low", "high"],
      thinkingModes: ["enabled", "disabled"],
      thinkingBudget: false,
      toolSupport: true,
      samplingSupport: true,
      source: "preset",
      verifiedAt: null,
      endpoint: "https://api.openai.com/v1/responses",
      parameterStyle: "openai",
    };
    assert.equal(
      helpers.modelParameterAvailability(model, capabilities).temperature,
      false,
    );
    assert.ok(helpers.modelParameterErrors(model, capabilities).temperature);
    assert.equal(model.defaults.temperature, 0.7);
  } finally {
    await server.close();
  }
});
