import {
  permissionOptions,
  type AgentSettingsController,
} from "../hooks/useAgentSettings";
/** 模型与权限管理复用首配表单，修改只影响下一轮运行。 */
export function AgentSettingsPanel({ s }: { s: AgentSettingsController }) {
  return (
    <section className="agent-settings">
      <div className="section-heading">
        <h3>AI Agent</h3>
        <button
          className="secondary"
          onClick={s.add}
          disabled={!s.enabled || s.busy}
        >
          添加 API 配置
        </button>
      </div>
      <p className="settings-description">
        连接自己的模型，读取项目、修改文本并协助 Git 操作。
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
      <label>
        当前模型配置
        <select
          value={s.view?.settings.activeProfileId ?? ""}
          onChange={s.select}
          disabled={!s.enabled || s.busy}
        >
          <option value="" disabled>
            请选择 API 配置
          </option>
          {s.view?.settings.profiles.map((profile) => (
            <option value={profile.id} key={profile.id}>
              {profile.name} · {profile.model}
            </option>
          ))}
        </select>
      </label>
      <div className="agent-profile-list">
        {s.view?.settings.profiles.map((profile) => (
          <div className="agent-profile" key={profile.id}>
            <div>
              <strong>{profile.name}</strong>
              <small>
                {profile.model} ·{" "}
                {s.view?.hasApiKeys[profile.id] ? "密钥已保存" : "密钥不可用"}
              </small>
              <small className="agent-path">{profile.baseUrl}</small>
            </div>
            <div className="button-row">
              <button
                className="secondary"
                disabled={s.busy}
                onClick={s.test(profile)}
                title="发送一次模型请求，可能消耗额度"
              >
                测试连接
              </button>
              <button
                className="secondary"
                disabled={s.busy}
                onClick={s.edit(profile)}
              >
                编辑
              </button>
              <button
                className="danger-text"
                disabled={s.busy}
                onClick={s.remove(profile)}
              >
                删除
              </button>
            </div>
          </div>
        ))}
      </div>
      {s.deletePending && (
        <div className="agent-delete-confirm">
          <p>删除配置“{s.deletePending.name}”及系统密钥？聊天记录会保留。</p>
          <div className="button-row">
            <button
              className="secondary"
              onClick={s.keepProfile}
              disabled={s.busy}
            >
              保留
            </button>
            <button
              className="danger-text"
              onClick={s.confirmDelete}
              disabled={s.busy}
            >
              确认删除配置
            </button>
          </div>
        </div>
      )}
      <label>
        AI 权限
        <select
          value={s.view?.settings.defaultPermissionMode ?? "requestApproval"}
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
            (s.view?.settings.defaultPermissionMode ?? "requestApproval"),
        )
        .map((option) => (
          <p className="settings-description" key={option.value}>
            {option.description} 更改在下一轮生效。
          </p>
        ))}
    </section>
  );
}
