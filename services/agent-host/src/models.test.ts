import assert from "node:assert/strict";
import { test } from "node:test";
import { validateProvider } from "./models.ts";

const provider = {
  providerId: "gateway",
  displayName: "测试网关",
  protocol: "openai-completions",
  baseUrl: "https://example.invalid/v1",
  credentialRef: "GITMASTER_AI_TEST",
  allowInsecureHttp: false,
  models: [
    {
      modelId: "fixture",
      displayName: "测试模型",
      contextWindow: 8192,
      maxOutputTokens: 1024,
    },
  ],
};

test("拒绝含凭据 URL、未确认明文 HTTP 和无效容量", () => {
  assert.throws(() =>
    validateProvider({
      ...provider,
      baseUrl: "https://secret@example.invalid/v1",
    }),
  );
  assert.throws(() =>
    validateProvider({ ...provider, baseUrl: "http://example.invalid/v1" }),
  );
  assert.throws(() =>
    validateProvider({
      ...provider,
      models: [{ ...provider.models[0], maxOutputTokens: 8192 }],
    }),
  );
});

test("拒绝普通配置中的明文密钥并接受三种协议", () => {
  assert.throws(() => validateProvider({ ...provider, apiKey: "不得存储" }));
  for (const protocol of [
    "openai-completions",
    "openai-responses",
    "anthropic-messages",
  ])
    assert.equal(
      validateProvider({ ...provider, protocol }).protocol,
      protocol,
    );
});
