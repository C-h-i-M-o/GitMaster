import type { AgentSettingsController } from "../hooks/useAgentSettings";
import { permissionOptions } from "../hooks/useAgentSettings";
import "./agentModels.css";

/** 按供应商展示多模型配置，并保留聊天入口使用的旧选择投影。 */
export function AgentSettingsPanel({ s }: { s: AgentSettingsController }) {
  const settings = s.view?.settings;
  const configuration = s.configuration;
  return (
    <section className="agent-settings">
      <div className="section-heading">
        <h3>AI Agent</h3>
        <button
          className="secondary"
          onClick={s.add}
          disabled={!s.enabled || s.busy}
        >
          添加供应商
        </button>
      </div>
      <p className="settings-description">
        连接自己的模型。每个供应商连接可保存多个模型，共用一份系统密钥。
      </p>
      {s.error && (
        <p role="alert" className="agent-error">
          {s.error}
        </p>
      )}
      {s.notice && <p role="status">{s.notice}</p>}
      {s.view?.warnings.map((warning) => (
        <p role="status" key={warning}>
          {warning}
        </p>
      ))}
      {settings && settings.providers.length > 0 && (
        <div className="agent-provider-list">
          {settings.providers.map((provider) => {
            const models = settings.models.filter(
              (model) => model.providerId === provider.id,
            );
            return (
              <article className="agent-provider-card" key={provider.id}>
                <div className="agent-provider-heading">
                  <div>
                    <strong>{provider.name}</strong>
                    <small className="agent-path">{provider.baseUrl}</small>
                    <small>
                      {s.view?.providerHasApiKeys[provider.id]
                        ? "密钥已保存在系统密钥库"
                        : "密钥不可用，请编辑连接"}
                    </small>
                  </div>
                  <div className="button-row">
                    <button
                      className="secondary"
                      onClick={configuration.editProvider(provider)}
                      disabled={s.busy}
                    >
                      编辑连接
                    </button>
                    <button
                      className="secondary"
                      onClick={configuration.addModel(provider)}
                      disabled={s.busy}
                    >
                      添加模型
                    </button>
                    <button
                      className="danger-text"
                      onClick={configuration.requestDeleteProvider(provider)}
                      disabled={s.busy}
                    >
                      删除连接
                    </button>
                  </div>
                </div>
                {models.length > 0 ? (
                  <ul className="agent-model-list">
                    {models.map((model) => (
                      <li className="agent-model-row" key={model.id}>
                        <label className="agent-model-enabled">
                          <input
                            type="checkbox"
                            checked={model.enabled}
                            onChange={configuration.toggleSavedModel(model)}
                            disabled={s.busy}
                          />
                          <span>
                            <strong>{model.label}</strong>
                            <small>
                              {model.modelId} · {model.protocol}
                              {model.capabilities.toolSupport === false &&
                                " · 明确不支持 Agent 工具"}
                            </small>
                          </span>
                        </label>
                        {settings.activeModelConfigId === model.id && (
                          <span className="agent-model-active">默认模型</span>
                        )}
                        <button
                          className="secondary"
                          onClick={configuration.editModel(model)}
                          disabled={s.busy}
                        >
                          编辑参数
                        </button>
                      </li>
                    ))}
                  </ul>
                ) : (
                  <p className="muted small">此连接尚未配置模型。</p>
                )}
              </article>
            );
          })}
        </div>
      )}
      {settings && settings.profiles.length > 0 && (
        <label>
          聊天默认模型
          <select
            value={settings.activeProfileId ?? ""}
            onChange={s.select}
            disabled={!s.enabled || s.busy}
          >
            {settings.profiles.map((profile) => (
              <option value={profile.id} key={profile.id}>
                {profile.name} · {profile.model}
              </option>
            ))}
          </select>
        </label>
      )}
      {configuration.deletePending && (
        <div className="agent-delete-confirm" role="alertdialog">
          <p>
            删除连接“{configuration.deletePending.name}”及其{" "}
            {settings?.models.filter(
              (model) => model.providerId === configuration.deletePending?.id,
            ).length ?? 0}{" "}
            个模型配置？聊天历史仍会保留。
          </p>
          <div className="button-row">
            <button
              className="secondary"
              onClick={configuration.keepProvider}
              disabled={s.busy}
            >
              保留
            </button>
            <button
              className="danger-text"
              onClick={configuration.confirmDeleteProvider}
              disabled={s.busy}
            >
              {s.busy ? "正在删除…" : "确认删除连接"}
            </button>
          </div>
        </div>
      )}
      <label>
        AI 权限
        <select
          value={settings?.defaultPermissionMode ?? "requestApproval"}
          onChange={s.permission}
          disabled={!s.enabled || s.busy}
        >
          {permissionOptions.map((option) => (
            <option key={option.value} value={option.value}>
              {option.label}
            </option>
          ))}
        </select>
      </label>
      {permissionOptions
        .filter(
          (option) =>
            option.value ===
            (settings?.defaultPermissionMode ?? "requestApproval"),
        )
        .map((option) => (
          <p className="settings-description" key={option.value}>
            {option.description} 更改在下一轮生效。
          </p>
        ))}
    </section>
  );
}
