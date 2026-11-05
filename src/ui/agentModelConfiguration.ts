import type { ChangeEvent } from "react";
import type {
  DiscoveredModel,
  ModelDraft,
  ModelParameters,
  ProviderDraft,
  ProviderPreset,
  PresetModel,
  SaveProvider,
} from "../types/agentModels";
import type {
  ModelConfig,
  ProviderConnection,
  ProviderProtocol,
} from "../types/agent";

export type ModelParameterField = keyof ModelParameters;
export type ModelParameterErrors = Partial<Record<ModelParameterField, string>>;
export interface ModelParameterEditorController {
  model: ModelDraft | null;
  modelCapabilities: ModelConfig["capabilities"];
  modelAvailability: Record<ModelParameterField, boolean> | null;
  modelErrors: ModelParameterErrors;
  modelField: (
    field: "modelId" | "label" | "protocol",
  ) => (event: ChangeEvent<HTMLInputElement | HTMLSelectElement>) => void;
  parameterField: (
    field: ModelParameterField,
  ) => (event: ChangeEvent<HTMLInputElement | HTMLSelectElement>) => void;
  restoreParameter: (field: ModelParameterField) => () => void;
}

/** 创建空模型默认值，null 均表示遵循供应商默认行为。 */
export function emptyModelParameters(): ModelParameters {
  return {
    contextLimit: null,
    maxOutputTokens: null,
    thinkingMode: null,
    reasoningEffort: null,
    thinkingBudgetTokens: null,
    temperature: null,
    topP: null,
  };
}

/** 未知模型默认使用 32768 本地上下文预算，输出仍由服务端默认决定。 */
export function unknownModelParameters(): ModelParameters {
  return { ...emptyModelParameters(), contextLimit: 32_768 };
}

/** 将预设连接和推荐模型转换为只存在内存中的新建草稿。 */
export function providerDraftForPreset(preset: ProviderPreset | null): {
  provider: ProviderDraft;
  models: ModelDraft[];
} {
  const recommended = preset?.models.find(
    (model) => model.modelId === preset.defaultModelId,
  );
  return {
    provider: {
      id: null,
      presetId: preset?.id ?? null,
      name: preset?.name ?? "",
      baseUrl: preset?.baseUrl ?? "",
      protocol: preset?.protocol ?? "chatCompletions",
      apiKey: null,
      allowStoredKeyForChangedHost: false,
    },
    models: recommended ? [modelDraftForPreset(recommended)] : [],
  };
}

/** 从预设模型复制经过核验的推荐参数，不改写服务端模型 ID。 */
export function modelDraftForPreset(model: PresetModel): ModelDraft {
  return {
    id: null,
    modelId: model.modelId,
    label: model.label,
    protocol: model.protocol,
    enabled: true,
    defaults: { ...model.defaults },
    expectedRevision: null,
  };
}

/** 将已保存连接转换为不含密钥的编辑草稿。 */
export function providerDraftForEdit(
  provider: ProviderConnection,
  protocol: ProviderProtocol,
): ProviderDraft {
  return {
    id: provider.id,
    presetId: provider.presetId,
    name: provider.name,
    baseUrl: provider.baseUrl,
    protocol,
    apiKey: null,
    allowStoredKeyForChangedHost: false,
  };
}

/** 未知模型只开放本地预算与输出限制，不臆测服务端能力。 */
export function unknownModelCapabilities(): ModelConfig["capabilities"] {
  return {
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
  };
}

/** 能力声明仅适用于返回该声明时的精确端点。 */
export function capabilitiesMatchEndpoint(
  capabilities: ModelConfig["capabilities"],
  baseUrl: string,
  protocol: ProviderProtocol,
): boolean {
  if (!capabilities.endpoint) return false;
  const normalize = (value: string): string | null => {
    try {
      const url = new URL(value);
      const suffix = {
        chatCompletions: "/chat/completions",
        responses: "/responses",
        anthropicMessages: "/messages",
      }[protocol];
      const base = `${url.origin}${url.pathname.replace(/\/+$/, "")}`;
      return base.endsWith(suffix) ? base : `${base}${suffix}`;
    } catch {
      return null;
    }
  };
  const expected = normalize(capabilities.endpoint);
  return expected !== null && expected === normalize(baseUrl);
}

/** 将已保存模型转换为可编辑草稿，完整保留当前参数。 */
export function modelDraftForEdit(model: ModelConfig): ModelDraft {
  return {
    id: model.id,
    modelId: model.modelId,
    label: model.label,
    protocol: model.protocol,
    enabled: model.enabled,
    defaults: { ...model.defaults },
    expectedRevision: model.revision,
  };
}

/** 为发现结果或手动 ID 建立新模型草稿；未知能力只保留默认参数。 */
export function modelDraftForDiscovered(
  model: DiscoveredModel,
  protocol: ProviderProtocol,
  preset: ProviderPreset | null,
): ModelDraft {
  const presetModel = preset?.models.find(
    (candidate) => candidate.modelId === model.modelId,
  );
  return {
    id: null,
    modelId: model.modelId,
    label: model.label,
    protocol,
    enabled: true,
    defaults: presetModel
      ? { ...presetModel.defaults }
      : emptyModelParameters(),
    expectedRevision: null,
  };
}

/** 未发现的手动模型保留稳定 ID，并只继承精确匹配的预设能力。 */
export function modelDraftForManual(
  modelId: string,
  protocol: ProviderProtocol,
  preset: ProviderPreset | null,
): ModelDraft {
  const presetModel = preset?.models.find(
    (candidate) => candidate.modelId === modelId,
  );
  return {
    id: null,
    modelId,
    label: presetModel?.label ?? modelId,
    protocol: presetModel?.protocol ?? protocol,
    enabled: true,
    defaults: presetModel
      ? { ...presetModel.defaults }
      : unknownModelParameters(),
    expectedRevision: null,
  };
}

/** 比较服务来源（协议、主机与端口）；无效地址按变化处理以保护旧 Key。 */
export function providerHostChanged(
  originalBaseUrl: string | null,
  nextBaseUrl: string,
): boolean {
  if (originalBaseUrl === null) return false;
  const origin = (value: string): string | null => {
    try {
      return new URL(value).origin.toLowerCase();
    } catch {
      return null;
    }
  };
  const originalHost = origin(originalBaseUrl);
  const nextHost = origin(nextBaseUrl);
  return (
    originalHost === null || nextHost === null || originalHost !== nextHost
  );
}

/** 返回本地密钥提示；后端仍须独立校验换主机授权。 */
export function providerKeyError(
  provider: ProviderDraft,
  originalBaseUrl: string | null,
  hasStoredKey: boolean,
): string | null {
  const hasNewKey = !!provider.apiKey?.trim();
  if (hasNewKey) return null;
  if (provider.id === null) return "首次连接请填写 API Key。";
  if (!hasStoredKey) return "找不到已保存的 API Key，请重新填写。";
  if (
    providerHostChanged(originalBaseUrl, provider.baseUrl) &&
    !provider.allowStoredKeyForChangedHost
  )
    return "连接主机已变化；请填写新 Key，或明确允许将已保存 Key 用于此主机。";
  return null;
}

/** 列出依据当前能力和参数组合可编辑的设置项。 */
export function modelParameterAvailability(
  model: ModelDraft,
  capabilities: ModelConfig["capabilities"],
): Record<ModelParameterField, boolean> {
  const thinking = model.defaults.thinkingMode !== "disabled";
  const effort = model.defaults.reasoningEffort;
  let temperature = capabilities.samplingSupport === true;
  let topP = capabilities.samplingSupport === true;
  if (capabilities.parameterStyle === "deepseek") {
    temperature = temperature && !thinking;
    topP = topP && (model.modelId === "deepseek-flash" ? thinking : !thinking);
  }
  if (
    capabilities.parameterStyle === "openai" &&
    effort !== null &&
    effort !== "none"
  ) {
    temperature = false;
    topP = false;
  }
  return {
    contextLimit: true,
    maxOutputTokens: true,
    thinkingMode: capabilities.thinkingModes.length > 0,
    reasoningEffort:
      capabilities.supportedEfforts.length > 0 &&
      model.defaults.thinkingMode !== "disabled",
    thinkingBudgetTokens:
      capabilities.thinkingBudget && model.defaults.thinkingMode === "enabled",
    temperature,
    topP,
  };
}

/** 解释能力或思考组合为何禁用某个参数控件。 */
export function modelParameterDisabledReason(
  field: ModelParameterField,
  model: ModelDraft,
  capabilities: ModelConfig["capabilities"],
): string {
  if (field === "reasoningEffort" && model.defaults.thinkingMode === "disabled")
    return "关闭思考时不能设置思考强度。";
  if (
    field === "thinkingBudgetTokens" &&
    model.defaults.thinkingMode !== "enabled"
  )
    return "仅在模型明确支持并启用思考后可设置独立预算。";
  if (field === "temperature" || field === "topP") {
    if (capabilities.samplingSupport !== true)
      return "模型未确认支持采样参数。";
    if (capabilities.parameterStyle === "deepseek") {
      const thinking = model.defaults.thinkingMode !== "disabled";
      if (field === "temperature" && thinking)
        return "DeepSeek 开启或默认思考模式时不接受 temperature。";
      if (field === "topP" && model.modelId === "deepseek-flash" && !thinking)
        return "DeepSeek Flash 仅在思考模式下支持 topP。";
      if (field === "topP" && model.modelId !== "deepseek-flash" && thinking)
        return "DeepSeek Pro 仅关闭思考时支持 topP。";
    }
    if (
      capabilities.parameterStyle === "openai" &&
      model.defaults.reasoningEffort !== null &&
      model.defaults.reasoningEffort !== "none"
    )
      return "启用推理时不能同时设置采样参数。";
  }
  return "当前模型未声明支持此参数。";
}

/** 对齐核心的预算与能力校验，返回字段级错误以阻止无效保存。 */
export function modelParameterErrors(
  model: ModelDraft,
  capabilities: ModelConfig["capabilities"],
): ModelParameterErrors {
  const values = model.defaults;
  const errors: ModelParameterErrors = {};
  const context = values.contextLimit ?? capabilities.contextWindow ?? 32_768;
  const output = values.maxOutputTokens ?? 4096;
  const reserve = Math.max(1024, Math.ceil(context / 20));
  if (
    !Number.isInteger(context) ||
    context < 2048 ||
    context > 16_777_216 ||
    (capabilities.contextWindow !== null &&
      context > capabilities.contextWindow) ||
    output < 1 ||
    output > 1_048_576 ||
    (capabilities.maxOutputTokens !== null &&
      output > capabilities.maxOutputTokens) ||
    output + reserve >= context
  ) {
    errors.contextLimit =
      "上下文或输出限制超出模型上限，或没有足够的输入预算。";
    errors.maxOutputTokens = errors.contextLimit;
  }
  if (
    values.thinkingMode !== null &&
    !capabilities.thinkingModes.includes(values.thinkingMode)
  )
    errors.thinkingMode = "当前模型未确认支持此思考模式，请恢复供应商默认。";
  if (
    values.reasoningEffort !== null &&
    !capabilities.supportedEfforts.includes(values.reasoningEffort)
  )
    errors.reasoningEffort = "当前模型未确认支持此思考强度，请恢复供应商默认。";
  if (values.thinkingMode === "disabled" && values.reasoningEffort !== null)
    errors.reasoningEffort = "关闭思考时不能同时指定思考强度。";
  if (values.thinkingBudgetTokens !== null) {
    if (
      !capabilities.thinkingBudget ||
      values.thinkingBudgetTokens < 1024 ||
      values.thinkingBudgetTokens >= output ||
      values.thinkingMode !== "enabled"
    )
      errors.thinkingBudgetTokens =
        "思考预算需由模型明确支持、启用思考，且小于最大输出限制。";
  }
  if (
    values.temperature !== null &&
    (!Number.isFinite(values.temperature) ||
      values.temperature < 0 ||
      values.temperature > 2)
  )
    errors.temperature = "temperature 必须在 0～2 范围内。";
  if (
    values.topP !== null &&
    (!Number.isFinite(values.topP) || values.topP < 0 || values.topP > 1)
  )
    errors.topP = "topP 必须在 0～1 范围内。";
  if (
    (values.temperature !== null || values.topP !== null) &&
    capabilities.samplingSupport !== true
  ) {
    if (values.temperature !== null)
      errors.temperature = "当前模型未确认支持采样参数，请恢复供应商默认。";
    if (values.topP !== null)
      errors.topP = "当前模型未确认支持采样参数，请恢复供应商默认。";
  }
  if (capabilities.parameterStyle === "deepseek") {
    const thinking = values.thinkingMode !== "disabled";
    if (thinking && values.temperature !== null)
      errors.temperature =
        "DeepSeek 开启思考时不能覆盖 temperature，请恢复供应商默认。";
    if (
      values.topP !== null &&
      (model.modelId === "deepseek-flash"
        ? !thinking || values.topP < 0.95
        : thinking)
    )
      errors.topP =
        model.modelId === "deepseek-flash"
          ? "DeepSeek Flash 思考模式下 topP 范围为 0.95～1。"
          : "DeepSeek Pro 仅关闭思考时可设置 topP。";
  }
  if (
    capabilities.parameterStyle === "openai" &&
    values.reasoningEffort !== null &&
    values.reasoningEffort !== "none"
  ) {
    if (values.temperature !== null)
      errors.temperature = "启用推理时不能同时覆盖采样参数。";
    if (values.topP !== null) errors.topP = "启用推理时不能同时覆盖采样参数。";
  }
  return errors;
}

/** 表单变化后仅接受仍属于当前请求的发现或连接测试结果。 */
export function isCurrentConfigurationResponse(
  activeRequestId: string | null,
  responseRequestId: string,
): boolean {
  return activeRequestId !== null && activeRequestId === responseRequestId;
}

/** 使用十六进制时间和递增序号生成后端可接受的短请求标识。 */
export function configurationRequestId(
  timestamp: number,
  sequence: number,
): string {
  return timestamp.toString(16) + "-" + sequence.toString(16);
}

/** 构造整条连接原子保存请求，包含当前连接下编辑的模型集合。 */
export function saveProviderRequest(
  provider: ProviderDraft,
  expectedRevision: number | null,
  models: ModelDraft[],
): SaveProvider {
  return {
    ...provider,
    expectedRevision,
    models: models.map((model) => ({
      ...model,
      defaults: { ...model.defaults },
    })),
  };
}

/** 显示强度的中文标签，同时保留实际供应商档位供辅助说明。 */
export function reasoningEffortLabel(value: string): string {
  const labels: Record<string, string> = {
    none: "关闭",
    low: "低",
    medium: "中",
    high: "高",
    xhigh: "极高",
    max: "最高",
  };
  return labels[value] ?? value;
}
