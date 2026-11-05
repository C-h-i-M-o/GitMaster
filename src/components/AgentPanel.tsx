import type { AgentController } from "../hooks/useAgent";
import { useAgentTranscript } from "../hooks/useAgentTranscript";
import type { AgentEvent, PermissionMode } from "../types/agent";
import { formatTranscriptValue } from "../ui/agentPresentation";
import { AgentPermissionControl } from "./AgentPermission";
import { AgentTurn } from "./AgentTurn";
import { AgentModelParameters } from "./AgentModelParameters";
import { AgentMemoryPanel } from "./AgentMemoryPanel";
import { AgentUsage } from "./AgentUsage";
import { AgentContextPanel } from "./AgentContextPanel";
import { latestPreparedContext } from "../ui/agentUsage";
import "./agentModels.css";

const emptyAgentEvents: AgentEvent[] = [];
/** 基础聊天工作台，审批卡直接展示后端绑定的真实目标与参数。 */
export function AgentPanel({ a }: { a: AgentController }) {
  const events = a.record?.events ?? emptyAgentEvents;
  const { turns, activeTurn } = useAgentTranscript(
    events,
    a.run?.runId ?? null,
  );
  const defaultPermission: PermissionMode =
    a.settings.view?.settings.defaultPermissionMode ?? "requestApproval";
  const workspace = a.record ? a.currentChatWorkspace : a.selectedWorkspace;
  const modelAttempts = events.flatMap((event) =>
    event.kind === "modelAttempt"
      ? [
          {
            sequence: event.sequence,
            purpose: event.purpose,
            attempt: event.attempt,
          },
        ]
      : [],
  );
  const preparedContext = latestPreparedContext(events);
  return (
    <section className="agent-panel" aria-label="AI Agent">
      <div className="agent-conversation">
        <header className="agent-conversation-header">
          <div>
            <strong>{a.record?.session.title ?? "从一个任务开始"}</strong>
            <small className="agent-path">
              {workspace?.rootPath ??
                a.record?.session.repositoryPath ??
                "未选择工作区"}
            </small>
            {workspace && (
              <small className="agent-path">
                默认输出：{workspace.writePath}
              </small>
            )}
          </div>
          <button className="secondary" onClick={a.settings.add}>
            配置 API
          </button>
          <AgentMemoryPanel
            enabled={a.settings.enabled}
            workspaceId={workspace?.id ?? a.workspaceId}
            chat={a.record?.session ?? null}
            running={a.run !== null}
            ensureChat={a.ensureChatForMemory}
            onChatUpdated={a.updateChatSession}
            onMemoriesChanged={a.context.refreshPreview}
          />
          <AgentUsage
            attempts={modelAttempts}
            context={preparedContext}
            label="本聊天用量与上下文"
          />
        </header>
        <div
          ref={a.messagesRoot}
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
          ) : a.enabledModels.length === 0 ? (
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
          {turns.map((turn) => (
            <AgentTurn
              key={turn.sessionId + ":" + turn.runId}
              turn={turn}
              workspaceId={a.record?.session.workspaceId ?? a.workspaceId}
              activeRun={turn.runId === a.run?.runId ? a.run : null}
              onMemorySaved={a.context.refreshPreview}
            />
          ))}
          {a.run && !activeTurn && (
            <p className="agent-turn-alert" role="status">
              {a.run.phase === "awaitingApproval"
                ? "等待审批"
                : a.run.phase === "executingTool"
                  ? "正在执行工具"
                  : "正在生成回复"}
            </p>
          )}
          {a.run?.approval && (
            <section className="agent-approval" aria-label="工具批准">
              <strong>批准本次操作？</strong>
              <p>{a.run.approval.reason}</p>
              <p className="agent-path">{a.run.approval.target}</p>
              <details>
                <summary>{a.run.approval.call.name} · 查看参数</summary>
                <pre>
                  {formatTranscriptValue(a.run.approval.call.arguments)}
                </pre>
              </details>
              {a.run.approval.preview !== null && (
                <details>
                  <summary>查看 Git 操作预览</summary>
                  <pre>{formatTranscriptValue(a.run.approval.preview)}</pre>
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
            <p role="status">
              此聊天关联的文件夹工作区已更换；历史仍可阅读，请选择有效工作区新建聊天。
            </p>
          )}
          {a.missingModel && (
            <p role="alert">
              此聊天使用的模型已删除或停用，请重新选择模型后保存聊天配置。
            </p>
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
            <AgentContextPanel
              context={a.context}
              enabled={a.settings.enabled}
              running={a.run !== null}
              workspaceId={a.record?.session.workspaceId ?? a.workspaceId}
              hasChat={a.record !== null}
              hasText={a.draft.trim().length > 0}
              configurationDirty={a.configurationDirty}
            />
            <select
              aria-label="当前聊天模型"
              value={a.configuration.modelConfigId ?? ""}
              onChange={a.changeModelConfig}
              disabled={a.settings.busy || a.configurationBusy || !!a.run}
            >
              {a.missingModel && a.record && (
                <option value={a.configuration.modelConfigId ?? ""} disabled>
                  模型已删除或停用 · 请重新选择
                </option>
              )}
              {a.recentModels.length > 0 && (
                <optgroup label="最近使用">
                  {a.recentModels.map((model) => (
                    <option value={model.id} key={model.id}>
                      {model.label} · {model.modelId}
                      {model.capabilities.toolSupport === false
                        ? "（明确不支持 Agent 工具）"
                        : ""}
                    </option>
                  ))}
                </optgroup>
              )}
              {a.otherModels.length > 0 && (
                <optgroup label="所有启用模型">
                  {a.otherModels.map((model) => (
                    <option value={model.id} key={model.id}>
                      {model.label} · {model.modelId}
                      {model.capabilities.toolSupport === false
                        ? "（明确不支持 Agent 工具）"
                        : ""}
                    </option>
                  ))}
                </optgroup>
              )}
            </select>
            <AgentPermissionControl
              mode={a.configuration.permissionMode ?? defaultPermission}
              disabled={a.settings.busy || a.configurationBusy || !!a.run}
              onChange={a.changePermissionMode}
            />
            {a.record && a.configurationDirty && (
              <button
                type="button"
                className="secondary"
                onClick={() => void a.saveChatConfiguration()}
                disabled={
                  a.configurationBusy ||
                  !!a.run ||
                  a.mismatched ||
                  a.missingModel
                }
              >
                {a.configurationBusy ? "保存中…" : "保存聊天配置"}
              </button>
            )}
            {a.configuration.modelConfigId && (
              <details className="agent-chat-parameters">
                <summary>思考与高级参数</summary>
                <AgentModelParameters
                  configuration={a.chatParameterEditor()}
                  readOnlyModel
                  disabled={!!a.run || a.configurationBusy}
                />
              </details>
            )}
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
        </form>
      </div>
    </section>
  );
}
