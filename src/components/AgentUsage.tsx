import type { ContextPreview } from "../types/agentRun";
import type { MemorySelection } from "../types/agentMemory";
import {
  summarizeModelAttempts,
  formatTokenSubtotal,
  formatTokenValue,
  modelAttemptPhaseLabel,
  modelAttemptPurposeLabel,
  type ModelAttemptRecord,
  type PreparedContext,
} from "../ui/agentUsage";
import { memoryExclusionReason } from "../ui/agentMemory";
import "./agentUsage.css";

/** 展示单轮或会话的真实供应商用量，以及最新一次上下文估算。 */
export function AgentUsage({
  attempts,
  context = null,
  label = "模型用量",
}: {
  attempts: readonly ModelAttemptRecord[];
  context?: PreparedContext | null;
  label?: string;
}) {
  const summary = summarizeModelAttempts(attempts);
  if (!summary.attempts.length && !context) return null;
  return (
    <details className="agent-usage">
      <summary>
        {label}
        {summary.completedAttempts > 0
          ? " · " + summary.completedAttempts + " 个请求"
          : ""}
      </summary>
      {summary.attempts.length > 0 && (
        <section aria-label="模型请求用量">
          <h4>请求记录</h4>
          <p>
            输入{" "}
            {summary.completedAttempts > 0
              ? formatTokenSubtotal(summary.totals.inputTokens)
              : "未知"}
            {" · 输出 "}
            {summary.completedAttempts > 0
              ? formatTokenSubtotal(summary.totals.outputTokens)
              : "未知"}
          </p>
          {summary.pendingAttempts > 0 && (
            <p role="status">
              {summary.pendingAttempts} 个请求仍在进行或尚无终态。
            </p>
          )}
          <ul>
            {summary.attempts.map(({ purpose, attempt }) => (
              <li key={attempt.id}>
                <strong>
                  {modelAttemptPurposeLabel(purpose)} · 第 {attempt.attempt} 次
                  · {modelAttemptPhaseLabel(attempt.phase)}
                </strong>
                <span>
                  输入 {formatTokenValue(attempt.usage?.inputTokens)} · 输出{" "}
                  {formatTokenValue(attempt.usage?.outputTokens)}
                </span>
                <span>
                  缓存读取 {formatTokenValue(attempt.usage?.cachedInputTokens)}{" "}
                  · 缓存创建{" "}
                  {formatTokenValue(attempt.usage?.cacheCreationInputTokens)} ·
                  推理 {formatTokenValue(attempt.usage?.reasoningTokens)}
                </span>
                {attempt.errorCode && (
                  <span>错误代码：{attempt.errorCode}</span>
                )}
              </li>
            ))}
          </ul>
          <small>
            缓存与推理计数是输入/输出的子项，不另加到总量；供应商未提供的计数显示为未知。此处不估算费用。
          </small>
        </section>
      )}
      {context && (
        <ContextPreviewView
          preview={context.preview}
          memories={context.memories}
          willCompact={context.willCompact}
        />
      )}
    </details>
  );
}

/** 格式化上下文估算、预算、压缩边界及记忆筛选结果。 */
function ContextPreviewView({
  preview,
  memories,
  willCompact,
}: {
  preview: ContextPreview;
  memories: MemorySelection;
  willCompact?: boolean;
}) {
  return (
    <section className="agent-context-preview" aria-label="最近上下文估算">
      <h4>最近请求上下文</h4>
      <p>
        请求输入估算 {preview.estimatedInputTokens.toLocaleString("zh-CN")} /
        输入预算 {preview.inputBudget.toLocaleString("zh-CN")} tokens
      </p>
      {willCompact !== undefined && (
        <p>
          {willCompact
            ? "发送前将触发一次上下文压缩。"
            : "当前上下文不需要压缩。"}
        </p>
      )}
      <p>
        本地上下文上限 {preview.contextLimit.toLocaleString("zh-CN")} · 输出预留{" "}
        {preview.outputReserve.toLocaleString("zh-CN")} · 安全余量{" "}
        {preview.safetyReserve.toLocaleString("zh-CN")}
      </p>
      <p>
        检查点：
        {preview.checkpointThrough === null
          ? "尚未压缩"
          : "消息序号 " + preview.checkpointThrough}
        ；截断工具结果：
        {preview.truncatedToolSequences.length
          ? preview.truncatedToolSequences.join("、")
          : "无"}
      </p>
      <p>
        已选择 {memories.selected.length} 条记忆，估算{" "}
        {memories.estimatedTokens.toLocaleString("zh-CN")} tokens。
      </p>
      {memories.selected.length > 0 && (
        <ul>
          {memories.selected.map((memory) => (
            <li key={memory.id}>
              {memory.scope === "user" ? "用户偏好" : "工作区记忆"}：
              {memory.text}
            </li>
          ))}
        </ul>
      )}
      {memories.excluded.length > 0 && (
        <ul className="muted">
          {memories.excluded.map((item) => (
            <li key={item.id}>
              {item.id}：{memoryExclusionReason(item.reason)}
            </li>
          ))}
        </ul>
      )}
      <small>
        上下文 token 数为请求估算值，不代表供应商精确统计；模型响应 usage
        单独显示。
      </small>
    </section>
  );
}
