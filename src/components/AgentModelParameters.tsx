import { SettingsAdvanced } from "./SettingsAdvanced";
import {
  modelParameterDisabledReason,
  reasoningEffortLabel,
  type ModelParameterEditorController,
} from "../ui/agentModelConfiguration";

/** 根据模型已核验能力展示参数控件，冲突值需用户显式恢复默认。 */
export function AgentModelParameters({
  configuration,
  readOnlyModel = false,
  disabled = false,
}: {
  configuration: ModelParameterEditorController;
  readOnlyModel?: boolean;
  disabled?: boolean;
}) {
  const model = configuration.model;
  if (!model) return <p className="muted">先选择一个模型。</p>;
  const capabilities = configuration.modelCapabilities;
  const available = configuration.modelAvailability;
  const errors = configuration.modelErrors;
  return (
    <section className="agent-model-editor">
      {!readOnlyModel && (
        <label>
          模型 ID
          <input
            value={model.modelId}
            onChange={configuration.modelField("modelId")}
            autoComplete="off"
            spellCheck={false}
            required
          />
        </label>
      )}
      {!readOnlyModel && (
        <label>
          显示名称
          <input
            value={model.label}
            onChange={configuration.modelField("label")}
            maxLength={160}
            required
          />
        </label>
      )}
      {!readOnlyModel && (
        <label>
          API 协议
          <select
            value={model.protocol}
            onChange={configuration.modelField("protocol")}
          >
            <option value="chatCompletions">OpenAI Chat Completions</option>
            <option value="responses">OpenAI Responses</option>
            <option value="anthropicMessages">Anthropic Messages</option>
          </select>
        </label>
      )}
      <SettingsAdvanced description="思考模式、强度与采样选项">
        <fieldset className="agent-model-parameters">
          <legend>模型默认参数</legend>
          {capabilities.source === "unknown" ? (
            <p className="muted small">
              未核验的模型能力；仅提供本地上下文预算和输出限制，不代表服务端保证。
            </p>
          ) : (
            <p className="muted small">
              已知上下文上限：
              {capabilities.contextWindow?.toLocaleString("zh-CN") ?? "未知"}；
              最大输出：
              {capabilities.maxOutputTokens?.toLocaleString("zh-CN") ?? "未知"}
              。
            </p>
          )}
          <label>
            本地上下文预算
            <input
              type="number"
              min={2048}
              max={Math.min(
                16_777_216,
                capabilities.contextWindow ?? 16_777_216,
              )}
              step={1}
              value={model.defaults.contextLimit ?? ""}
              onChange={configuration.parameterField("contextLimit")}
              disabled={disabled}
            />
            <small>
              留空时采用模型上限；未知模型使用 32768 的保守本地预算。
            </small>
            {errors.contextLimit && (
              <span className="agent-model-field-error" role="alert">
                {errors.contextLimit}
                <button
                  type="button"
                  className="secondary"
                  onClick={configuration.restoreParameter("contextLimit")}
                  disabled={disabled}
                >
                  恢复默认
                </button>
              </span>
            )}
          </label>
          <label>
            最大输出 token
            <input
              type="number"
              min={1}
              max={Math.min(
                1_048_576,
                capabilities.maxOutputTokens ?? 1_048_576,
              )}
              step={1}
              value={model.defaults.maxOutputTokens ?? ""}
              onChange={configuration.parameterField("maxOutputTokens")}
              disabled={disabled}
            />
            {errors.maxOutputTokens && (
              <span className="agent-model-field-error" role="alert">
                {errors.maxOutputTokens}
                <button
                  type="button"
                  className="secondary"
                  onClick={configuration.restoreParameter("maxOutputTokens")}
                  disabled={disabled}
                >
                  恢复默认
                </button>
              </span>
            )}
          </label>
          {(capabilities.thinkingModes.length > 0 ||
            model.defaults.thinkingMode !== null) && (
            <label>
              思考模式
              <select
                value={model.defaults.thinkingMode ?? ""}
                onChange={configuration.parameterField("thinkingMode")}
                disabled={disabled || !available?.thinkingMode}
              >
                <option value="">供应商默认</option>
                {model.defaults.thinkingMode !== null &&
                  !capabilities.thinkingModes.includes(
                    model.defaults.thinkingMode,
                  ) && (
                    <option value={model.defaults.thinkingMode} disabled>
                      当前模型不支持此模式
                    </option>
                  )}
                {capabilities.thinkingModes.map((mode) => (
                  <option key={mode} value={mode}>
                    {mode === "enabled" ? "启用" : "关闭"}
                  </option>
                ))}
              </select>
              {errors.thinkingMode && (
                <span className="agent-model-field-error" role="alert">
                  {errors.thinkingMode}
                  <button
                    type="button"
                    className="secondary"
                    onClick={configuration.restoreParameter("thinkingMode")}
                    disabled={disabled}
                  >
                    恢复默认
                  </button>
                </span>
              )}
            </label>
          )}
          {(capabilities.supportedEfforts.length > 0 ||
            model.defaults.reasoningEffort !== null) && (
            <label>
              思考强度
              <select
                value={model.defaults.reasoningEffort ?? ""}
                onChange={configuration.parameterField("reasoningEffort")}
                disabled={disabled || !available?.reasoningEffort}
              >
                <option value="">供应商默认</option>
                {model.defaults.reasoningEffort !== null &&
                  !capabilities.supportedEfforts.includes(
                    model.defaults.reasoningEffort,
                  ) && (
                    <option value={model.defaults.reasoningEffort} disabled>
                      当前模型不支持此档位
                    </option>
                  )}
                {capabilities.supportedEfforts.map((effort) => (
                  <option key={effort} value={effort}>
                    {reasoningEffortLabel(effort)}（{effort}）
                  </option>
                ))}
              </select>
              {!available?.reasoningEffort && (
                <small>
                  {modelParameterDisabledReason(
                    "reasoningEffort",
                    model,
                    capabilities,
                  )}
                </small>
              )}
              {errors.reasoningEffort && (
                <span className="agent-model-field-error" role="alert">
                  {errors.reasoningEffort}
                  <button
                    type="button"
                    className="secondary"
                    onClick={configuration.restoreParameter("reasoningEffort")}
                    disabled={disabled}
                  >
                    恢复默认
                  </button>
                </span>
              )}
            </label>
          )}
          {(capabilities.thinkingBudget ||
            model.defaults.thinkingBudgetTokens !== null) && (
            <label>
              思考预算 token
              <input
                type="number"
                min={1024}
                max={(model.defaults.maxOutputTokens ?? 4096) - 1}
                step={1}
                value={model.defaults.thinkingBudgetTokens ?? ""}
                onChange={configuration.parameterField("thinkingBudgetTokens")}
                disabled={disabled || !available?.thinkingBudgetTokens}
              />
              {!available?.thinkingBudgetTokens && (
                <small>
                  {modelParameterDisabledReason(
                    "thinkingBudgetTokens",
                    model,
                    capabilities,
                  )}
                </small>
              )}
              {errors.thinkingBudgetTokens && (
                <span className="agent-model-field-error" role="alert">
                  {errors.thinkingBudgetTokens}
                  <button
                    type="button"
                    className="secondary"
                    onClick={configuration.restoreParameter(
                      "thinkingBudgetTokens",
                    )}
                    disabled={disabled}
                  >
                    恢复默认
                  </button>
                </span>
              )}
            </label>
          )}
          {(capabilities.samplingSupport === true ||
            model.defaults.temperature !== null) && (
            <label>
              temperature
              <input
                type="number"
                min={0}
                max={2}
                step={0.1}
                value={model.defaults.temperature ?? ""}
                onChange={configuration.parameterField("temperature")}
                disabled={disabled || !available?.temperature}
              />
              {!available?.temperature && (
                <small>
                  {modelParameterDisabledReason(
                    "temperature",
                    model,
                    capabilities,
                  )}
                </small>
              )}
              {errors.temperature && (
                <span className="agent-model-field-error" role="alert">
                  {errors.temperature}
                  <button
                    type="button"
                    className="secondary"
                    onClick={configuration.restoreParameter("temperature")}
                    disabled={disabled}
                  >
                    恢复默认
                  </button>
                </span>
              )}
            </label>
          )}
          {(capabilities.samplingSupport === true ||
            model.defaults.topP !== null) && (
            <label>
              topP
              <input
                type="number"
                min={
                  capabilities.parameterStyle === "deepseek" &&
                  model.modelId === "deepseek-flash"
                    ? 0.95
                    : 0
                }
                max={1}
                step={0.01}
                value={model.defaults.topP ?? ""}
                onChange={configuration.parameterField("topP")}
                disabled={disabled || !available?.topP}
              />
              {!available?.topP && (
                <small>
                  {modelParameterDisabledReason("topP", model, capabilities)}
                </small>
              )}
              {errors.topP && (
                <span className="agent-model-field-error" role="alert">
                  {errors.topP}
                  <button
                    type="button"
                    className="secondary"
                    onClick={configuration.restoreParameter("topP")}
                    disabled={disabled}
                  >
                    恢复默认
                  </button>
                </span>
              )}
            </label>
          )}
        </fieldset>
      </SettingsAdvanced>
    </section>
  );
}
