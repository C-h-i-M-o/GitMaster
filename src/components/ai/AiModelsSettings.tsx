import { useAiModels } from "../../hooks/useAiModels";
import { useAiTheme } from "../../hooks/useAiTheme";
import css from "./AiModels.module.css";
import "./upstream-theme.css";
import "./ai.css";

/** 复用上游提供商卡片和编辑器样式，事件与密钥状态由 hook 持有。 */
export function AiModelsSettings() {
  const f = useAiModels();
  const dark = useAiTheme();
  return (
    <div
      className="ai-surface ai-model-settings"
      data-ai-dark={dark ? "" : undefined}
    >
      <section className={css.section}>
        <h3 className={css.title}>模型</h3>
        <p className={css.intro}>配置自己的模型 API，在会话中选择模型。</p>
        {!f.desktop && <p role="status">请在桌面应用中配置模型。</p>}
        {f.error && (
          <p className={css.notice} role="alert">
            {f.error}
            <button type="button" onClick={f.reload} disabled={f.busy}>
              重新读取配置（保留草稿）
            </button>
          </p>
        )}
        {f.notice && (
          <p className={css.savedNotice} role="status">
            {f.notice}
          </p>
        )}
        <ul className={css.rows}>
          {f.snapshot?.settings.providers.map((provider) => (
            <li key={provider.providerId} className={css.rowCard}>
              <div className={css.rowHead}>
                <div className={css.rowIdentity}>
                  <strong className={css.rowName}>
                    {provider.displayName}
                  </strong>
                  <span className={css.rowTag}>{provider.protocol}</span>
                  <span
                    className={`${css.credentialDot} ${f.snapshot?.credentials[provider.providerId] === "configured" ? css.credentialDotConfigured : css.credentialDotMissing}`}
                    title={
                      f.snapshot?.credentials[provider.providerId] ===
                      "configured"
                        ? "密钥已配置"
                        : "密钥未配置或不可用"
                    }
                  />
                </div>
                <div className={css.rowActions}>
                  <button
                    className={css.secondaryButton}
                    disabled={f.busy}
                    onClick={f.testConnection(provider)}
                    title="将向已保存模型发送一个最小请求，可能产生 API 费用"
                  >
                    测试连接
                  </button>
                  <button
                    className={css.secondaryButton}
                    disabled={f.busy}
                    onClick={f.edit(provider)}
                  >
                    编辑
                  </button>
                  <button
                    className={css.dangerButton}
                    disabled={f.busy}
                    onClick={f.remove(provider)}
                  >
                    删除
                  </button>
                </div>
              </div>
              <p className="ai-caption">{provider.baseUrl}</p>
              <div className="ai-model-tags">
                {provider.models.map((model) => (
                  <span key={model.modelId}>{model.displayName}</span>
                ))}
              </div>
            </li>
          ))}
        </ul>
        {f.draft ? (
          <fieldset className={css.editor} disabled={f.busy || !f.desktop}>
            <div className={css.editorHeader}>
              <strong className={css.editorTitle}>
                {f.editing ? "编辑模型提供商" : "自定义模型 API"}
              </strong>
            </div>
            <div className="ai-model-grid">
              <label className={css.field}>
                <span className={css.fieldLabel}>Provider ID</span>
                <input
                  className={css.input}
                  maxLength={64}
                  value={f.draft.providerId}
                  onChange={f.field("providerId")}
                  disabled={f.editing}
                  placeholder="my-gateway"
                  spellCheck={false}
                />
              </label>
              <label className={css.field}>
                <span className={css.fieldLabel}>显示名称</span>
                <input
                  className={css.input}
                  maxLength={200}
                  value={f.draft.displayName}
                  onChange={f.field("displayName")}
                  placeholder="模型提供商名称"
                />
              </label>
            </div>
            <label className={css.field}>
              <span className={css.fieldLabel}>API 协议</span>
              <select
                className={`${css.input} ${css.selectInput}`}
                value={f.draft.protocol}
                onChange={f.protocol}
              >
                <option value="openai-completions">
                  OpenAI Chat Completions
                </option>
                <option value="openai-responses">OpenAI Responses</option>
                <option value="anthropic-messages">Anthropic Messages</option>
              </select>
            </label>
            <label className={css.field}>
              <span className={css.fieldLabel}>基础 URL</span>
              <input
                className={css.input}
                value={f.draft.baseUrl}
                onChange={f.field("baseUrl")}
                placeholder="https://gateway.example/v1"
                spellCheck={false}
              />
            </label>
            <label className="ai-checkbox">
              <input
                type="checkbox"
                checked={f.draft.allowInsecureHttp}
                onChange={f.insecure}
              />
              我确认允许连接非本机的明文 HTTP 地址
            </label>
            <label className={css.field}>
              <span className={css.fieldLabel}>API 密钥</span>
              <div className="ai-key-field">
                <input
                  className={css.input}
                  type={f.showKey ? "text" : "password"}
                  autoComplete="off"
                  value={f.key}
                  onChange={f.changeKey}
                  placeholder={
                    f.draft.credentialRef
                      ? "留空使用已保存凭据"
                      : "输入 API 密钥；本地无认证服务可留空"
                  }
                />
                {f.key && (
                  <button
                    type="button"
                    className={css.secondaryButton}
                    onClick={f.toggleKey}
                  >
                    {f.showKey ? "隐藏" : "显示"}
                  </button>
                )}
              </div>
            </label>
            {f.draft.credentialRef && (
              <button
                className={css.dangerButton}
                type="button"
                onClick={f.clearKey}
              >
                清除已保存密钥{f.removeKey ? "（保存后生效）" : ""}
              </button>
            )}
            <div className="ai-model-catalog">
              <strong>模型目录</strong>
              {f.draft.models.map((model, index) => (
                <div className="ai-model-entry" key={index}>
                  <div className="ai-model-grid">
                    <label className={css.field}>
                      <span className={css.fieldLabel}>模型 ID</span>
                      <input
                        className={css.input}
                        maxLength={200}
                        value={model.modelId}
                        onChange={f.modelField(index, "modelId")}
                        placeholder="实际模型 ID"
                        spellCheck={false}
                      />
                    </label>
                    <label className={css.field}>
                      <span className={css.fieldLabel}>显示名称</span>
                      <input
                        className={css.input}
                        maxLength={200}
                        value={model.displayName}
                        onChange={f.modelField(index, "displayName")}
                        placeholder="默认使用模型 ID"
                      />
                    </label>
                  </div>
                  <div className="ai-model-grid">
                    <label className={css.field}>
                      <span className={css.fieldLabel}>上下文窗口</span>
                      <input
                        className={css.input}
                        type="number"
                        min="1"
                        value={model.contextWindow || ""}
                        onChange={f.modelField(index, "contextWindow")}
                        placeholder="实际 token 容量"
                      />
                    </label>
                    <label className={css.field}>
                      <span className={css.fieldLabel}>最大输出 token</span>
                      <input
                        className={css.input}
                        type="number"
                        min="1"
                        value={model.maxOutputTokens || ""}
                        onChange={f.modelField(index, "maxOutputTokens")}
                        placeholder="小于上下文容量"
                      />
                    </label>
                  </div>
                  <button
                    type="button"
                    className={css.dangerButton}
                    disabled={f.draft!.models.length <= 1}
                    onClick={f.removeModel(index)}
                  >
                    移除模型
                  </button>
                </div>
              ))}
              <button
                type="button"
                className={css.secondaryButton}
                onClick={f.addModel}
              >
                ＋ 添加模型
              </button>
            </div>
            <div className="ai-form-actions">
              <button
                type="button"
                className={css.secondaryButton}
                onClick={f.cancel}
              >
                取消
              </button>
              <button
                type="button"
                className={css.primaryButton}
                onClick={f.save}
              >
                {f.busy ? "正在保存…" : "保存"}
              </button>
            </div>
          </fieldset>
        ) : (
          <button
            className={css.addButton}
            disabled={!f.desktop || f.busy}
            onClick={f.add}
          >
            ＋ 添加模型提供商
          </button>
        )}
      </section>
    </div>
  );
}
