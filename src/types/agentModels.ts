import type { ProviderProtocol } from "./agent";

/** 默认值为 null，表示不覆盖供应商行为。 */
export type ThinkingMode = "enabled" | "disabled";
export interface ModelParameters {
  contextLimit: number | null;
  maxOutputTokens: number | null;
  thinkingMode: ThinkingMode | null;
  reasoningEffort: string | null;
  thinkingBudgetTokens: number | null;
  temperature: number | null;
  topP: number | null;
}
/** 能力来源与协议映射由后端核验，未知值不伪装成支持。 */
export interface ModelCapabilities {
  contextWindow: number | null;
  maxOutputTokens: number | null;
  supportedEfforts: string[];
  thinkingModes: ThinkingMode[];
  thinkingBudget: boolean;
  toolSupport: boolean | null;
  samplingSupport: boolean | null;
  source: "preset" | "discovery" | "unknown";
  verifiedAt: string | null;
  endpoint: string | null;
  parameterStyle:
    "deepseek" | "openai" | "anthropicAdaptive" | "anthropicBudget" | "generic";
}
export interface PresetModel {
  modelId: string;
  label: string;
  protocol: ProviderProtocol;
  capabilities: ModelCapabilities;
  defaults: ModelParameters;
}
export interface ProviderPreset {
  id: string;
  name: string;
  baseUrl: string;
  protocol: ProviderProtocol;
  defaultModelId: string;
  models: PresetModel[];
  verifiedAt: string;
}
/** 密钥只存在于编辑草稿和一次 IPC 内，不能持久化到前端缓存。 */
export interface ProviderDraft {
  id: string | null;
  presetId: string | null;
  name: string;
  baseUrl: string;
  protocol: ProviderProtocol;
  apiKey: string | null;
  allowStoredKeyForChangedHost: boolean;
}
export interface ModelDraft {
  id: string | null;
  modelId: string;
  label: string;
  protocol: ProviderProtocol;
  enabled: boolean;
  defaults: ModelParameters;
  expectedRevision: number | null;
}
/** 保存连接及本次编辑的模型为同一个事务；未列出的模型保持原状。 */
export interface SaveProvider extends ProviderDraft {
  expectedRevision: number | null;
  models: ModelDraft[];
}
export interface SaveModel extends ModelDraft {
  providerId: string;
}
export interface DiscoveredModel {
  modelId: string;
  label: string;
  capabilities: ModelCapabilities;
}
export interface DiscoveredModelPage {
  requestId: string;
  models: DiscoveredModel[];
  nextCursor: string | null;
}
export interface ConnectionTestReport {
  requestId: string;
  stages: { id: string; status: "passed" | "unsupported"; message: string }[];
}
