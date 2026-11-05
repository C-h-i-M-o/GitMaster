import { memo } from "react";
import type { RunSnapshot } from "../types/agent";
import {
  formatTranscriptValue,
  runPhase,
  transcriptRunStatus,
  transcriptToolStatus,
  transcriptToolSummary,
} from "../ui/agentPresentation";
import type {
  AgentTranscriptTool,
  AgentTranscriptTurn,
} from "../ui/agentPresentation";
import { AgentPermissionBadge } from "./AgentPermission";
import {
  AgentChanges,
  AgentRunConfiguration,
  AgentRunWarnings,
} from "./AgentChanges";
import { AgentTextBlock } from "./AgentTextBlock";
import { AgentUsage } from "./AgentUsage";
import { AgentRunContext } from "./AgentRunContext";
import "./AgentTranscript.css";

/** 展示完整工具调用、审批事实及其配对结果。 */
function AgentToolCard({ tool }: { tool: AgentTranscriptTool }) {
  const needsAttention =
    tool.isError || (tool.approval !== null && tool.approved === null);
  return (
    <details
      className={`agent-tool-card ${tool.isError ? "is-error" : ""}`}
      open={needsAttention}
    >
      <summary>
        <strong>{tool.name}</strong>
        <span>{transcriptToolStatus(tool)}</span>
      </summary>
      <div className="agent-tool-content">
        {tool.approval && (
          <section className="agent-tool-approval" aria-label="审批记录">
            <strong>
              {tool.approved === null
                ? "等待批准"
                : tool.approved
                  ? "用户已批准"
                  : "用户未批准"}
            </strong>
            <p>{tool.approval.reason}</p>
            <p className="agent-path">{tool.approval.target}</p>
          </section>
        )}
        {tool.arguments !== null && (
          <section>
            <strong>调用参数</strong>
            <AgentTextBlock
              text={formatTranscriptValue(tool.arguments)}
              className="agent-tool-value"
            />
          </section>
        )}
        {tool.completed && (
          <section>
            <strong>{tool.isError ? "失败详情" : "执行结果"}</strong>
            <AgentTextBlock
              text={formatTranscriptValue(tool.result)}
              className="agent-tool-value"
            />
          </section>
        )}
        {tool.evidence.map((item, index) => (
          <section key={`${tool.callId}-evidence-${index}`}>
            <strong>总结证据</strong>
            <AgentTextBlock
              text={formatTranscriptValue(item.evidence)}
              className="agent-tool-value"
            />
            {item.relatedChats.length > 0 && (
              <small>关联聊天：{item.relatedChats.join("、")}</small>
            )}
          </section>
        ))}
      </div>
    </details>
  );
}

/** 独立渲染变化频繁的模型实时文本。 */
const AgentTurnLive = memo(function AgentTurnLiveView({
  text,
}: {
  text: string;
}) {
  return (
    <section className="agent-turn-live" aria-live="polite">
      <small>正在生成</small>
      <div className="agent-message-text">{text}</div>
    </section>
  );
});

/** 只依赖稳定的持久投影，避免流式片段重绘过程历史。 */
const AgentTurnStatic = memo(function AgentTurnStaticView({
  turn,
  workspaceId,
  onMemorySaved,
}: {
  turn: AgentTranscriptTurn;
  workspaceId: string;
  onMemorySaved?: () => void;
}) {
  return (
    <>
      {turn.hasError && (
        <section className="agent-turn-alert" role="alert">
          <strong>本轮有操作失败</strong>
          {turn.errorMessage && <p>{turn.errorMessage}</p>}
          {turn.tools
            .filter((tool) => tool.isError)
            .map((tool) => (
              <p key={tool.callId}>工具“{tool.name}”执行失败，详情见过程卡。</p>
            ))}
        </section>
      )}
      {turn.hasPendingApproval && (
        <section className="agent-turn-alert" role="status">
          <strong>有操作等待审批</strong>
          <p>
            {turn.tools
              .filter((tool) => tool.approval && tool.approved === null)
              .map((tool) => tool.name)
              .join("、")}
            ；审批记录尚未完成。
          </p>
        </section>
      )}
      <AgentRunWarnings warnings={turn.warnings} />
      {turn.hasProcess && (
        <details
          className="agent-turn-process"
          open={turn.hasAttention || !turn.ended}
        >
          <summary>
            <span>处理过程</span>
            <small>{transcriptToolSummary(turn)}</small>
          </summary>
          <div className="agent-turn-process-content">
            <AgentUsage attempts={turn.modelAttempts} label="本轮用量" />
            <AgentRunContext
              chatId={turn.sessionId}
              workspaceId={workspaceId}
              suggestions={turn.memorySuggestions}
              referenceGroups={turn.references}
              onMemorySaved={onMemorySaved}
            />
            <AgentRunConfiguration configurations={turn.configurations} />
            <AgentChanges chatId={turn.sessionId} changes={turn.changes} />
            {turn.processMessages.map((message) => (
              <section
                className="agent-turn-assistant-step"
                key={message.sequence}
                id={`agent-message-${turn.sessionId}-${message.sequence}`}
                data-agent-sequence={message.sequence}
              >
                {message.thinking.map((thinking, index) => (
                  <details
                    className="agent-turn-thinking"
                    key={`thought-${index}`}
                  >
                    <summary>供应商返回的思考摘要</summary>
                    <AgentTextBlock text={thinking} />
                  </details>
                ))}
                {message.text && (
                  <AgentTextBlock
                    text={message.text}
                    className="agent-message-text"
                  />
                )}
              </section>
            ))}
            {turn.tools.map((tool) => (
              <AgentToolCard key={tool.callId} tool={tool} />
            ))}
          </div>
        </details>
      )}
      {turn.finalMessage && (
        <section
          className="agent-turn-final"
          id={`agent-message-${turn.sessionId}-${turn.finalMessage.sequence}`}
          data-agent-sequence={turn.finalMessage.sequence}
        >
          <small>GitMaster</small>
          <AgentTextBlock text={turn.finalMessage.text} />
          {turn.finalMessage.thinking.map((thinking, index) => (
            <details
              className="agent-turn-thinking"
              key={`final-thought-${index}`}
            >
              <summary>供应商返回的思考摘要</summary>
              <AgentTextBlock text={thinking} />
            </details>
          ))}
        </section>
      )}
    </>
  );
});

/** 将静态持久轮次与动态运行状态分开显示。 */
const AgentTurn = memo(function AgentTurnView({
  turn,
  activeRun,
  workspaceId,
  onMemorySaved,
}: {
  turn: AgentTranscriptTurn;
  activeRun: RunSnapshot | null;
  workspaceId: string;
  onMemorySaved?: () => void;
}) {
  return (
    <article className="agent-turn" data-status={turn.status}>
      <header className="agent-turn-header">
        <strong>{turn.userMessage ? "用户消息" : "运行续聊"}</strong>
        <span>
          {activeRun ? runPhase(activeRun.phase) : transcriptRunStatus(turn)}
        </span>
      </header>
      {turn.permissionMode && (
        <AgentPermissionBadge mode={turn.permissionMode} />
      )}
      {turn.userMessage && (
        <section
          className="agent-turn-user"
          id={
            turn.userSequence === null
              ? undefined
              : `agent-message-${turn.sessionId}-${turn.userSequence}`
          }
          data-agent-sequence={turn.userSequence ?? undefined}
        >
          <small>你</small>
          <AgentTextBlock text={turn.userMessage} />
        </section>
      )}
      <AgentTurnStatic
        turn={turn}
        workspaceId={workspaceId}
        onMemorySaved={onMemorySaved}
      />
      {activeRun && !turn.ended && activeRun.text.length > 0 && (
        <AgentTurnLive text={activeRun.text} />
      )}
    </article>
  );
});

export { AgentTurn };
