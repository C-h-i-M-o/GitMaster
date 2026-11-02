import type { AgentSettingsController } from "../hooks/useAgentSettings";
import { useNativeDialog } from "../hooks/useNativeDialog";
/** 首次使用与后续编辑共用配置表单，密钥不回填。 */
export function AgentSetupDialog({ s }: { s: AgentSettingsController }) {
  const dialog = useNativeDialog(s.setupOpen, s.closeSetup);
  return (
    <dialog
      ref={dialog.dialogRef}
      onCancel={dialog.onCancel}
      className="gm-dialog agent-setup"
      aria-labelledby="agent-setup-title"
    >
      <header className="drawer-header">
        <h2 id="agent-setup-title">
          {s.draft.id ? "编辑 AI 配置" : "连接你的 AI"}
        </h2>
        <button
          onClick={s.closeSetup}
          disabled={s.busy}
          aria-label="关闭 AI 配置"
        >
          ×
        </button>
      </header>
      <form className="dialog-body agent-form" onSubmit={s.save}>
        {!s.enabled && <p role="status">请在桌面应用中保存配置。</p>}
        <p className="muted">
          使用自己的模型服务。密钥保存在系统密钥库；对话历史保存在此设备。发送消息时，相关对话和工具结果会交给你配置的服务。
        </p>
        <label>
          配置名称
          <input
            value={s.draft.name}
            onChange={s.field("name")}
            placeholder="例如：日常开发"
            required
            maxLength={100}
            disabled={!s.enabled || s.busy}
          />
        </label>
        <label>
          API 协议
          <select
            value={s.draft.protocol}
            onChange={s.protocolChanged}
            disabled={!s.enabled || s.busy}
          >
            <option value="chatCompletions">OpenAI Chat Completions</option>
            <option value="responses">OpenAI Responses</option>
            <option value="anthropicMessages">Anthropic Messages</option>
          </select>
        </label>
        <label>
          Base URL
          <input
            value={s.draft.baseUrl}
            onChange={s.field("baseUrl")}
            type="url"
            required
            autoComplete="off"
            spellCheck={false}
            disabled={!s.enabled || s.busy}
          />
        </label>
        <label>
          模型 ID
          <input
            value={s.draft.model}
            onChange={s.field("model")}
            placeholder="填写服务商提供的模型 ID"
            required
            autoComplete="off"
            spellCheck={false}
            disabled={!s.enabled || s.busy}
          />
        </label>
        <label>
          API Key
          {s.draft.id && <small className="muted">留空保留已保存的密钥</small>}
          <input
            type="password"
            value={s.draft.apiKey ?? ""}
            onChange={s.field("apiKey")}
            required={!s.draft.id}
            autoComplete="new-password"
            spellCheck={false}
            disabled={!s.enabled || s.busy}
          />
        </label>
        {s.error && (
          <p role="alert" className="agent-error">
            {s.error}
          </p>
        )}
        <div className="button-row">
          <button
            type="button"
            className="secondary"
            onClick={s.closeSetup}
            disabled={s.busy}
          >
            暂不配置
          </button>
          <button
            className="primary"
            type="submit"
            disabled={!s.enabled || s.busy}
          >
            {s.busy ? "正在保存…" : "保存配置"}
          </button>
        </div>
      </form>
    </dialog>
  );
}
