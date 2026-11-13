import { useAiSandbox } from "../../hooks/useAiSandbox";
import { Icon } from "../Icon";

/** 当前项目 ACL 诊断与明确确认预览，不能从会话或模型触发。 */
export function AiSandboxSettings({
  repositoryId,
}: {
  repositoryId: string | null;
}) {
  const f = useAiSandbox(repositoryId);
  return (
    <section className="settings-sandbox" aria-label="终端沙箱权限">
      <div className="settings-sandbox-heading">
        <div>
          <h3>终端沙箱权限</h3>
          <p className="settings-description">{f.description}</p>
        </div>
        <button
          className="secondary"
          disabled={!repositoryId || f.busy}
          onClick={f.check}
        >
          <Icon name="refresh" />
          {f.checkLabel}
        </button>
      </div>
      <div
        className="settings-sandbox-status"
        data-state={f.statusTone}
        role="status"
        aria-live="polite"
      >
        <Icon
          name={
            f.statusTone === "ready"
              ? "check"
              : f.statusTone === "loading"
                ? "refresh"
                : "activity"
          }
        />
        <div>
          <strong>{f.statusLabel}</strong>
          {f.detail && <p>{f.detail}</p>}
        </div>
      </div>
      {f.status && (
        <p className="settings-path settings-sandbox-path">
          <span>当前项目</span>
          {f.displayRoot}
        </p>
      )}
      {f.status && !f.status.ready && f.status.shellAvailable && (
        <button className="secondary" disabled={f.busy} onClick={f.prepare}>
          查看修复方案
        </button>
      )}
      {f.repair && (
        <div className="ai-approval">
          <strong>确认修复此项目权限？</strong>
          <p className="settings-path">{f.repair.root}</p>
          <p>{f.repair.detail}</p>
          <p className="settings-path">备份：{f.repair.backupPath}</p>
          <button className="secondary" disabled={f.busy} onClick={f.cancel}>
            取消
          </button>
          <button className="primary" disabled={f.busy} onClick={f.execute}>
            确认修复当前项目
          </button>
        </div>
      )}
      {f.error && (
        <p className="settings-sandbox-error" role="alert">
          {f.error}
        </p>
      )}
      {f.result && <pre className="settings-sandbox-result">{f.result}</pre>}
    </section>
  );
}
