import type { useLogSettings } from "../hooks/useLogSettings";
import { describeGitError } from "../ui/gitPresentation";

/** 展示日志详细程度与保存状态，不向日志发送用户输入。 */
export function LogSettingsPanel({
  state,
}: {
  state: ReturnType<typeof useLogSettings>;
}) {
  return (
    <section className="preference-section">
      <h3>诊断日志</h3>
      <label>
        日志级别
        <select
          aria-label="日志级别"
          value={state.settings?.level ?? "auto"}
          onChange={state.change}
          disabled={state.busy || !state.settings}
        >
          <option value="auto">跟随环境（推荐）</option>
          <option value="error">Error · 仅错误</option>
          <option value="warn">Warn · 错误和警告</option>
          <option value="info">Info · 常规操作</option>
          <option value="debug">Debug · 调试详情</option>
          <option value="trace">Trace · 完整诊断</option>
        </select>
      </label>
      <p className="muted small">
        选择后立即保存并生效。正式版默认 Error，开发版默认
        Trace；越详细的级别包含越多诊断信息。
      </p>
      {state.settings && (
        <>
          <p>当前生效：{state.settings.effectiveLevel.toUpperCase()}</p>
          {!state.settings.available && (
            <p role="alert">
              日志文件初始化失败，请检查日志目录权限后重启应用。
            </p>
          )}
          <p className="muted small">
            不记录凭据、仓库路径、文件内容或 Git 原始输出。单文件 5 MiB，保留 3
            个归档。
          </p>
          <p className="muted small">{state.settings.directory}</p>
          <button
            className="secondary"
            disabled={state.busy}
            onClick={state.openDirectory}
          >
            打开日志目录
          </button>
        </>
      )}
      {state.busy && <p role="status">正在处理日志设置…</p>}
      {state.error && (
        <div role="alert">
          <p>{describeGitError(state.error)}</p>
          <button
            className="secondary"
            disabled={state.busy}
            onClick={state.reload}
          >
            重新读取设置
          </button>
        </div>
      )}
    </section>
  );
}
