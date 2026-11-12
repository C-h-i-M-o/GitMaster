import { useAiSandbox } from "../../hooks/useAiSandbox";

/** 当前项目 ACL 诊断与明确确认预览，不能从会话或模型触发。 */
export function AiSandboxSettings({
  repositoryId,
}: {
  repositoryId: string | null;
}) {
  const f = useAiSandbox(repositoryId);
  return (
    <section>
      <h3>AI 终端</h3>
      <p className="settings-description">
        检查当前项目的 Windows 沙箱权限。检查不修改目录权限。
      </p>
      {!repositoryId && <p>请先打开项目。</p>}
      <button disabled={!repositoryId || f.busy} onClick={f.check}>
        检查当前项目
      </button>
      {f.status && (
        <>
          <p role="status">
            {f.status.ready
              ? "沙箱权限已就绪"
              : `沙箱不可用：${f.status.operation}（Win32 ${f.status.win32Code ?? "未知"}）`}
          </p>
          <p className="settings-path">{f.status.root}</p>
          {!f.status.ready && f.status.shellAvailable && (
            <button disabled={f.busy} onClick={f.prepare}>
              查看修复方案
            </button>
          )}
        </>
      )}
      {f.repair && (
        <div className="ai-approval">
          <strong>确认修复此项目权限？</strong>
          <p className="settings-path">{f.repair.root}</p>
          <p>{f.repair.detail}</p>
          <p className="settings-path">备份：{f.repair.backupPath}</p>
          <button disabled={f.busy} onClick={f.cancel}>
            取消
          </button>
          <button disabled={f.busy} onClick={f.execute}>
            确认修复当前项目
          </button>
        </div>
      )}
      {f.error && <p role="alert">{f.error}</p>}
      {f.result && <pre>{f.result}</pre>}
    </section>
  );
}
