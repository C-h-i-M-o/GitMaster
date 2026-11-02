import type { AgentSettingsController } from "../hooks/useAgentSettings";
import { useDeviceStorage } from "../hooks/useDeviceStorage";
import { agentBytes } from "../ui/agentPresentation";
/** 以真实字节分类展示此设备数据，错误不伪装为零占用。 */
export function DeviceStoragePanel({ s }: { s: AgentSettingsController }) {
  useDeviceStorage(s);
  return (
    <section className="agent-storage">
      <div className="section-heading">
        <h3>此设备</h3>
        <button
          className="secondary"
          disabled={!s.enabled || s.usageBusy}
          onClick={s.refreshUsage}
        >
          {s.usageBusy ? "读取中…" : "刷新"}
        </button>
      </div>
      {s.usageError && (
        <p role="alert" className="agent-error">
          {s.usageError}
        </p>
      )}
      {s.usage && (
        <>
          <p className="agent-storage-total">
            {agentBytes(s.usage.totalBytes)} <small>本地数据总量</small>
          </p>
          <div className="agent-storage-bar" aria-hidden="true">
            {s.usage.categories.map((item, index) => (
              <span
                key={item.id}
                className={`storage-color-${index}`}
                style={{ flexGrow: item.bytes }}
              />
            ))}
          </div>
          <dl className="agent-storage-list">
            {s.usage.categories.map((item, index) => (
              <div key={item.id}>
                <dt>
                  <i className={`storage-color-${index}`} />
                  {item.label}
                </dt>
                <dd>{agentBytes(item.bytes)}</dd>
              </div>
            ))}
          </dl>
          <details>
            <summary>数据存储位置</summary>
            {s.usage.locations.map((path) => (
              <p className="agent-path" key={path}>
                {path}
              </p>
            ))}
          </details>
          <p className="settings-description">
            聊天及归档可在 AI Agent 中管理。缓存可在下方缓存设置中清除。
          </p>
        </>
      )}
    </section>
  );
}
