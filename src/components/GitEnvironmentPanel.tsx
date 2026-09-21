import type { GitEnvironmentState } from "../hooks/useGitEnvironment";
import { describeGitError } from "../ui/gitPresentation";
interface Props extends GitEnvironmentState {
  onRefresh: () => void;
  onChoose: () => void;
  onReset: () => void;
  onInstall: () => void;
  choosing: boolean;
}
/** 展示真实 Git 环境、错误及恢复入口，预览模式不提供伪操作。 */
export function GitEnvironmentPanel({
  environment,
  error,
  status,
  onRefresh,
  onChoose,
  onReset,
  onInstall,
  choosing,
}: Props) {
  const disabled = status === "preview" || status === "loading" || choosing;
  return (
    <section
      className="foundation-panel"
      aria-labelledby="git-title"
      aria-busy={status === "loading"}
    >
      <div className="panel-heading">
        <span className="small-symbol" aria-hidden="true">
          01
        </span>
        <div>
          <h2 id="git-title">你的 Git 环境</h2>
          <p>使用电脑上的 Git，不自动安装或修改系统设置。</p>
        </div>
      </div>
      {status === "preview" ? (
        <p role="status">界面预览 · 请在桌面应用中使用 Git 功能。</p>
      ) : (
        <>
          {status === "loading" && <p role="status">正在检测 Git…</p>}
          {environment?.status === "ready" && (
            <div className="git-ready">
              <strong>Git {environment.version}</strong>
              <code>{environment.executablePath}</code>
              <small>
                {environment.source === "manual" ? "手动设置" : "自动检测"}
              </small>
            </div>
          )}
          {error && <p role="alert">{describeGitError(error)}</p>}
        </>
      )}
      <div className="panel-actions">
        <button type="button" disabled={disabled} onClick={onRefresh}>
          重新检测
        </button>
        <button type="button" disabled={disabled} onClick={onChoose}>
          选择 Git
        </button>
        <button type="button" disabled={disabled} onClick={onReset}>
          恢复自动检测
        </button>
        <button
          type="button"
          disabled={status === "preview"}
          onClick={onInstall}
        >
          打开安装官网
        </button>
      </div>
    </section>
  );
}
