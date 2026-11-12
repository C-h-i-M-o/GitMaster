import type { AiProvider } from "./contracts.ts";
import type { PiAiProviderProfile } from "@deepseek-ai/dsh-llm-pi-ai";
import { z } from "zod";

const modelSchema = z
  .strictObject({
    modelId: z.string().trim().min(1).max(200),
    displayName: z.string().trim().min(1).max(200),
    contextWindow: z.number().int().positive().max(Number.MAX_SAFE_INTEGER),
    maxOutputTokens: z.number().int().positive(),
    reasoningEfforts: z
      .record(z.string().min(1), z.string().nullable())
      .optional(),
  })
  .refine(
    (model) => model.maxOutputTokens < model.contextWindow,
    "输出上限必须小于上下文容量",
  );

const providerSchema = z
  .strictObject({
    providerId: z.string().regex(/^[a-z][a-z0-9-]{0,63}$/),
    displayName: z.string().trim().min(1).max(200),
    protocol: z.enum([
      "openai-completions",
      "openai-responses",
      "anthropic-messages",
    ]),
    baseUrl: z.url().max(2048),
    credentialRef: z
      .string()
      .regex(/^[A-Z_][A-Z0-9_]*$/)
      .nullable(),
    allowInsecureHttp: z.boolean(),
    models: z.array(modelSchema).min(1).max(100),
  })
  .refine((provider) => {
    const url = new URL(provider.baseUrl);
    return (
      !url.username &&
      !url.password &&
      !url.search &&
      !url.hash &&
      (url.protocol === "https:" ||
        (url.protocol === "http:" &&
          (provider.allowInsecureHttp ||
            ["localhost", "127.0.0.1", "[::1]"].includes(url.hostname))))
    );
  }, "API 地址包含凭据或尚未确认明文连接")
  .refine(
    (provider) =>
      new Set(provider.models.map((model) => model.modelId)).size ===
      provider.models.length,
    "模型 ID 不可重复",
  );

/** 验证提供商配置，后续连接层不直接接纳不可信对象。 */
export function validateProvider(input: unknown): AiProvider {
  return providerSchema.parse(input);
}

/** 将用户配置转换为上游适配器 profile，不把密钥混入配置。 */
export function providerProfiles(
  providers: readonly AiProvider[],
): Record<string, PiAiProviderProfile> {
  return Object.fromEntries(
    providers.map((input) => {
      const provider = validateProvider(input);
      return [
        provider.providerId,
        {
          displayName: provider.displayName,
          api: provider.protocol,
          baseURL: provider.baseUrl,
          apiKeyEnv: provider.credentialRef ?? "GITMASTER_AI_NO_KEY",
          retryPolicy: { mode: "normal" as const, maxRetries: 2 },
          models: provider.models.map((model) => ({
            id: model.modelId,
            name: model.displayName,
            contextWindow: model.contextWindow,
            maxTokens: model.maxOutputTokens,
            input: ["text" as const],
            ...(model.reasoningEfforts
              ? {
                  reasoningEfforts: Object.fromEntries(
                    Object.entries(model.reasoningEfforts).map(
                      ([key, value]) => [key, value ?? undefined],
                    ),
                  ),
                }
              : {}),
          })),
        },
      ];
    }),
  );
}
