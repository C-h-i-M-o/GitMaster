import type { AgentController } from "../hooks/useAgent";
import { permissionOptions } from "../hooks/useAgentSettings";
import { eventPresentation, runPhase } from "../ui/agentPresentation";
import type { AgentEvent } from "../types/agent";
/** 展示持久事件；仓库或网页内容始终作为纯文本，不执行 HTML。 */
function AgentMessage({ event }: { event: AgentEvent }) {
  const item = eventPresentation(event);
  if (!item) return null;
  return (
    <article className={`agent-message agent-message-${item.kind}`}>
      <small>{item.role}</small>
      {item.details ? (
        <details>
          <summary>{item.text}</summary>
          <pre>{item.details}</pre>
        </details>
      ) : (
        <div className="agent-message-text">{item.text}</div>
      )}
    </article>
  );
}
/** 基础聊天工作台，审批卡直接展示后端绑定的真实目标与参数。 */
export function AgentPanel({ a }: { a: AgentController }) {
  return (
    <section className="agent-panel" aria-label="AI Agent">
      <div className="agent-conversation">
        <header className="agent-conversation-header">
          <div>
            <strong>{a.record?.session.title ?? "从一个任务开始"}</strong>
            <small className="agent-path">
              {a.record?.session.repositoryPath ??
                a.repositoryPath ??
                "未打开项目"}
            </small>
          </div>
          <button className="secondary" onClick={a.settings.add}>
            配置 API
          </button>
        </header>
        <div
          className="agent-messages"
          onScroll={a.scroll}
          tabIndex={0}
          aria-label="对话内容"
        >
          {a.settings.view === null ? (
            <p className="agent-empty" role="status">
              {a.settings.error
                ? "暂时无法读取模型配置。"
                : "正在读取模型配置…"}
            </p>
          ) : !a.settings.view.settings.activeProfileId ? (
            <div className="agent-empty">
              <span className="eyebrow">AI AGENT</span>
              <h2>先连接一个 AI 模型</h2>
              <p>配置模型后即可在此与项目上下文协作。</p>
              <button className="primary" onClick={a.settings.add}>
                配置 AI 模型
              </button>
            </div>
          ) : !a.record?.events.length ? (
            <div className="agent-empty">
              <span className="eyebrow">PROJECT ASSISTANT</span>
              <h2>让项目事实成为回答的依据</h2>
              <p>了解改动、阅读文件，或根据指定提交生成开发总结。</p>
              <p className="muted">
                例如：查看当前 Git 状态，解释这些修改。
                <br />
                请根据提交 SHA … 生成开发总结。
              </p>
            </div>
          ) : null}
          {a.record?.events.map((event) => (
            <AgentMessage key={event.sequence} event={event} />
          ))}
          {a.run?.text && (
            <article className="agent-message agent-message-assistant">
              <small>GitMaster · 正在生成</small>
              <div className="agent-message-text">{a.run.text}</div>
            </article>
          )}
          {a.run?.approval && (
            <section className="agent-approval" aria-label="工具批准">
              <strong>批准本次操作？</strong>
              <p>{a.run.approval.reason}</p>
              <p className="agent-path">{a.run.approval.target}</p>
              <details>
                <summary>{a.run.approval.call.name} · 查看参数</summary>
                <pre>
                  {JSON.stringify(a.run.approval.call.arguments, null, 2)}
                </pre>
              </details>
              {a.run.approval.preview !== null && (
                <details>
                  <summary>查看 Git 操作预览</summary>
                  <pre>{JSON.stringify(a.run.approval.preview, null, 2)}</pre>
                </details>
              )}
              <div className="button-row">
                <button
                  className="secondary"
                  onClick={a.resolve(false)}
                  disabled={a.busy}
                >
                  拒绝
                </button>
                <button
                  className="primary"
                  onClick={a.resolve(true)}
                  disabled={a.busy}
                >
                  批准一次
                </button>
              </div>
            </section>
          )}
          <div ref={a.end} />
        </div>
        <form className="agent-composer" onSubmit={a.send}>
          {a.error && (
            <p role="alert" className="agent-error">
              {a.error}
            </p>
          )}
          {a.settings.error && !a.settings.setupOpen && (
            <p role="alert" className="agent-error">
              {a.settings.error}
            </p>
          )}
          {a.mismatched && (
            <p role="status">请先打开此聊天关联的项目，或新建聊天。</p>
          )}
          {a.record?.session.archived && (
            <p role="status">聊天已归档，恢复后可以继续。</p>
          )}
          <label className="sr-only" htmlFor="agent-message">
            给 AI 的消息
          </label>
          <textarea
            id="agent-message"
            value={a.draft}
            onChange={a.changeDraft}
            onKeyDown={a.keyDown}
            onKeyUp={a.keyUp}
            rows={3}
            onBlur={a.composerBlur}
            placeholder="描述任务… Enter 发送，Ctrl / ⌘ / Shift + Enter 换行"
            disabled={a.disabled}
          />
          <div className="agent-composer-toolbar">
            <select
              aria-label="本轮模型配置"
              value={a.settings.view?.settings.activeProfileId ?? ""}
              onChange={a.settings.select}
              disabled={a.settings.busy || !!a.run}
            >
              <option value="" disabled>
                选择模型
              </option>
              {a.settings.view?.settings.profiles.map((profile) => (
                <option value={profile.id} key={profile.id}>
                  {profile.name} · {profile.model}
                </option>
              ))}
            </select>
            <select
              aria-label="AI 权限"
              value={
                a.settings.view?.settings.defaultPermissionMode ??
                "requestApproval"
              }
              onChange={a.settings.permission}
              disabled={a.settings.busy || !!a.run}
            >
              {permissionOptions.map((option) => (
                <option value={option.value} key={option.value}>
                  {option.label}
                </option>
              ))}
            </select>
            {a.run ? (
              <button
                type="button"
                className="secondary"
                onClick={a.cancel}
                disabled={a.busy || a.run.phase === "stopping"}
              >
                停止
              </button>
            ) : (
              <button
                type="submit"
                className="primary"
                disabled={a.disabled || !a.draft.trim()}
              >
                发送
              </button>
            )}
          </div>
          {a.run && (
            <p role="status" className="small muted">
              {runPhase(a.run.phase)}
            </p>
          )}
        </form>
      </div>
    </section>
  );
}
