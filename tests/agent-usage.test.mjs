import assert from "node:assert/strict";
import test from "node:test";
import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { createServer } from "vite";

test("用量按 attempt ID 去重，保留已知小计并标出未知调用", async () => {
  const server = await createServer({
    server: { middlewareMode: true, hmr: false },
    appType: "custom",
  });
  try {
    const { summarizeModelAttempts } = await server.ssrLoadModule(
      "/src/ui/agentUsage.ts",
    );
    const result = summarizeModelAttempts([
      {
        sequence: 2,
        purpose: "reply",
        attempt: {
          id: "a1",
          attempt: 1,
          phase: "completed",
          usage: {
            inputTokens: 100,
            outputTokens: 20,
            cachedInputTokens: 30,
            cacheCreationInputTokens: 5,
            reasoningTokens: 4,
          },
          errorCode: null,
        },
      },
      {
        sequence: 1,
        purpose: "reply",
        attempt: {
          id: "a1",
          attempt: 1,
          phase: "started",
          usage: null,
          errorCode: null,
        },
      },
      {
        sequence: 3,
        purpose: "reply",
        attempt: {
          id: "a1",
          attempt: 1,
          phase: "completed",
          usage: {
            inputTokens: 100,
            outputTokens: 20,
            cachedInputTokens: 30,
            cacheCreationInputTokens: 5,
            reasoningTokens: 4,
          },
          errorCode: null,
        },
      },
      {
        sequence: 4,
        purpose: "reply",
        attempt: {
          id: "a2",
          attempt: 2,
          phase: "failed",
          usage: {
            inputTokens: 50,
            outputTokens: 10,
            cachedInputTokens: null,
            cacheCreationInputTokens: null,
            reasoningTokens: null,
          },
          errorCode: "RATE_LIMITED",
        },
      },
      {
        sequence: 5,
        purpose: "reply",
        attempt: {
          id: "a3",
          attempt: 3,
          phase: "failed",
          usage: null,
          errorCode: "NETWORK",
        },
      },
    ]);

    assert.equal(result.attempts.length, 3);
    assert.equal(result.attempts[0].attempt.phase, "completed");
    assert.equal(result.totals.inputTokens.subtotal, 150);
    assert.equal(result.totals.inputTokens.unknownAttempts, 1);
    assert.equal(result.totals.outputTokens.subtotal, 30);
    assert.equal(result.totals.outputTokens.unknownAttempts, 1);
    assert.equal(result.totals.cachedInputTokens.subtotal, 30);
    assert.equal(result.totals.cachedInputTokens.unknownAttempts, 2);
  } finally {
    await server.close();
  }
});

test("超出安全整数的供应商计数被标为未知，不发生精度溢出", async () => {
  const server = await createServer({
    server: { middlewareMode: true, hmr: false },
    appType: "custom",
  });
  try {
    const { summarizeModelAttempts } = await server.ssrLoadModule(
      "/src/ui/agentUsage.ts",
    );
    const result = summarizeModelAttempts([
      {
        sequence: 1,
        purpose: "reply",
        attempt: {
          id: "overflow",
          attempt: 1,
          phase: "completed",
          usage: {
            inputTokens: Number.MAX_SAFE_INTEGER,
            outputTokens: 2,
            cachedInputTokens: 0,
            cacheCreationInputTokens: 0,
            reasoningTokens: 0,
          },
          errorCode: null,
        },
      },
      {
        sequence: 2,
        purpose: "reply",
        attempt: {
          id: "overflow-2",
          attempt: 2,
          phase: "completed",
          usage: {
            inputTokens: 1,
            outputTokens: -1,
            cachedInputTokens: 0,
            cacheCreationInputTokens: 0,
            reasoningTokens: 0,
          },
          errorCode: null,
        },
      },
    ]);

    assert.equal(result.totals.inputTokens.subtotal, Number.MAX_SAFE_INTEGER);
    assert.equal(result.totals.inputTokens.unknownAttempts, 1);
    assert.equal(result.totals.outputTokens.subtotal, 2);
    assert.equal(result.totals.outputTokens.unknownAttempts, 1);
  } finally {
    await server.close();
  }
});

test("用量组件明确区分真实响应计数与上下文估算", async () => {
  const server = await createServer({
    server: { middlewareMode: true, hmr: false },
  });
  try {
    const { AgentUsage } = await server.ssrLoadModule(
      "/src/components/AgentUsage.tsx",
    );
    const markup = renderToStaticMarkup(
      React.createElement(AgentUsage, {
        attempts: [
          {
            purpose: "reply",
            attempt: {
              id: "request-1",
              attempt: 1,
              phase: "completed",
              usage: {
                inputTokens: 120,
                outputTokens: 30,
                cachedInputTokens: 40,
                cacheCreationInputTokens: null,
                reasoningTokens: null,
              },
              errorCode: null,
            },
          },
        ],
        context: {
          preview: {
            estimatedInputTokens: 110,
            inputBudget: 2000,
            contextLimit: 4096,
            outputReserve: 1024,
            safetyReserve: 1024,
            checkpointThrough: null,
            truncatedToolSequences: [],
          },
          memories: { selected: [], excluded: [], estimatedTokens: 0 },
        },
      }),
    );

    assert.match(markup, /输入 120 · 输出 30/);
    assert.match(markup, /缓存读取 40 · 缓存创建 未知 · 推理 未知/);
    assert.match(markup, /请求输入估算 110 \/\s*输入预算 2,000 tokens/);
    assert.match(markup, /不代表供应商精确统计/);
    assert.match(markup, /不估算费用/);
  } finally {
    await server.close();
  }
});
