import { useNativeDialog } from "../hooks/useNativeDialog";
import type { AgentSettingsController } from "../hooks/useAgentSettings";
import { AgentModelParameters } from "./AgentModelParameters";

/** 展示连接、模型、目录发现和未保存连接测试的设置表单。 */
export function AgentSetupDialog({ s }: { s: AgentSettingsController }) {
  const c = s.configuration;
  const dialog = useNativeDialog(c.setupOpen, c.close);
  const preset = c.selectedPreset;
  return (
    <dialog
      ref={dialog.dialogRef}
      onCancel={dialog.onCancel}
      className="gm-dialog agent-setup"
      aria-labelledby="agent-setup-title"
    >
      <header className="drawer-header">
        <h2 id="agent-setup-title">
          {c.mode === "model"
            ? "模型配置"
            : c.provider.id
              ? "编辑 AI 连接"
              : "连接 AI 服务"}
        </h2>
        <button
          type="button"
          onClick={c.close}
          disabled={c.busy}
          aria-label="关闭 AI 配置"
        >
          ×
        </button>
      </header>
      <form className="dialog-body agent-form" onSubmit={c.submit}>
        <p className="muted">
          API Key 仅保存在当前编辑草稿和本次请求中；测试连接不会保存配置。
        </p>
        {c.mode === "provider" && (
          <>
            <label>
              服务预设
              <select
                value={c.provider.presetId ?? ""}
                onChange={c.presetChanged}
              >
                <option value="">自定义兼容服务</option>
                {s.view?.presets.map((item) => (
                  <option key={item.id} value={item.id}>
                    {item.name}
                  </option>
                ))}
              </select>
            </label>
            <label>
              连接名称
              <input
                value={c.provider.name}
                onChange={c.field("name")}
                required
                maxLength={120}
              />
            </label>
            <label>
              API 协议
              <select value={c.provider.protocol} onChange={c.protocolChanged}>
                <option value="chatCompletions">OpenAI Chat Completions</option>
                <option value="responses">OpenAI Responses</option>
                <option value="anthropicMessages">Anthropic Messages</option>
              </select>
            </label>
            <label>
              服务地址
              <input
                type="url"
                value={c.provider.baseUrl}
                onChange={c.field("baseUrl")}
                required
                autoComplete="url"
              />
            </label>
            <label>
              API Key
              <input
                type="password"
                value={c.provider.apiKey ?? ""}
                onChange={c.field("apiKey")}
                autoComplete="new-password"
                placeholder={
                  c.hasStoredKey ? "已保存密钥；留空使用原密钥" : "填写 API Key"
                }
              />
            </label>
            {c.storedKeyConsentRequired && (
              <label className="agent-consent">
                <input
                  type="checkbox"
                  checked={c.provider.allowStoredKeyForChangedHost}
                  onChange={c.allowStoredKeyChanged}
                />
                明确允许将已保存的 API Key 用于更改后的服务地址
              </label>
            )}
            {c.keyError && (
              <p className="agent-model-field-error" role="alert">
                {c.keyError}
              </p>
            )}
            <section className="agent-connection-models">
              <h3>连接中的模型</h3>
              {preset?.models
                .filter(
                  (item) =>
                    !c.models.some((model) => model.modelId === item.modelId),
                )
                .map((item) => (
                  <label key={item.modelId} className="agent-model-choice">
                    <input
                      type="checkbox"
                      onChange={c.presetModelChanged(item)}
                    />
                    {item.label}
                    <small>{item.modelId}</small>
                  </label>
                ))}
              {c.models.map((model) => (
                <div
                  key={model.modelId}
                  className={`agent-model-choice${c.model?.modelId === model.modelId ? " is-selected" : ""}`}
                >
                  <button
                    type="button"
                    className="agent-model-select"
                    onClick={c.selectModel(model)}
                  >
                    {model.label} <small>{model.modelId}</small>
                  </button>
                  <label>
                    <input
                      type="checkbox"
                      checked={model.enabled}
                      onChange={c.modelEnabledChanged(model)}
                    />
                    启用
                  </label>
                </div>
              ))}
            </section>
            {c.model && <AgentModelParameters configuration={c} />}
            <section className="agent-model-discovery">
              <h3>发现或手动添加模型</h3>
              <div className="agent-inline-actions">
                <button
                  type="button"
                  className="secondary"
                  onClick={() => void c.discoverModels()}
                  disabled={!s.enabled || c.busy || c.discoveryBusy}
                >
                  发现模型
                </button>
                {c.discoveryBusy && (
                  <button
                    type="button"
                    className="secondary"
                    onClick={c.cancelDiscovery}
                  >
                    取消
                  </button>
                )}
                {c.discoveryCursor && (
                  <button
                    type="button"
                    className="secondary"
                    onClick={() => void c.discoverModels(true)}
                    disabled={c.discoveryBusy}
                  >
                    加载更多
                  </button>
                )}
              </div>
              {c.discoveryError && (
                <p className="agent-model-field-error" role="alert">
                  {c.discoveryError}
                </p>
              )}
              <label>
                搜索发现结果
                <input value={c.search} onChange={c.searchChanged} />
              </label>
              {c.discoveryModels.map((item) => (
                <label key={item.modelId} className="agent-model-choice">
                  <input
                    type="checkbox"
                    checked={c.models.some(
                      (model) => model.modelId === item.modelId,
                    )}
                    onChange={c.discoveredModelChanged(item)}
                  />
                  {item.label}
                  <small>{item.modelId}</small>
                </label>
              ))}
              <label>
                手动模型 ID
                <input
                  value={c.manualModelId}
                  onChange={c.manualModelChanged}
                  onKeyDown={(event) => {
                    if (event.key === "Enter") {
                      event.preventDefault();
                      c.addManualModel();
                    }
                  }}
                />
              </label>
              <button
                type="button"
                className="secondary"
                onClick={c.addManualModel}
              >
                添加模型 ID
              </button>
            </section>
          </>
        )}
        {c.mode === "model" && (
          <>
            <p className="muted">
              连接：
              {s.view?.settings.providers.find(
                (item) => item.id === c.provider.id,
              )?.name ?? "未知连接"}
              。{c.hasStoredKey ? "已有密钥" : "需要补充密钥"}
            </p>
            <button
              type="button"
              className="secondary"
              onClick={c.editCurrentProvider}
            >
              编辑连接或密钥
            </button>
            <AgentModelParameters configuration={c} />
          </>
        )}
        {(c.localError || s.error || c.testError) && (
          <p className="agent-model-field-error" role="alert">
            {c.localError ?? c.testError ?? s.error}
          </p>
        )}
        {c.testReport && (
          <section className="agent-test-report" aria-live="polite">
            <h3>连接测试结果</h3>
            {c.testReport.stages.map((stage) => (
              <p key={stage.id}>
                <strong>{stage.status === "passed" ? "通过" : "不支持"}</strong>
                ：{stage.message}
              </p>
            ))}
          </section>
        )}
        <div className="agent-setup-actions">
          <button
            type="button"
            className="secondary"
            onClick={() => void c.testCurrentModel()}
            disabled={!s.enabled || c.busy || c.testBusy || !c.model}
          >
            测试未保存配置
          </button>
          <small className="muted">
            测试可能产生模型调用费用；工具协议探针或重试可能调用多次。
          </small>
          {c.testBusy && (
            <button type="button" className="secondary" onClick={c.cancelTest}>
              取消测试
            </button>
          )}
          <button
            type="button"
            className="secondary"
            onClick={c.close}
            disabled={c.busy}
          >
            取消
          </button>
          <button type="submit" disabled={!s.enabled || c.busy}>
            {c.busy ? "保存中…" : "保存"}
          </button>
        </div>
      </form>
    </dialog>
  );
}
