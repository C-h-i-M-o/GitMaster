import type { AgentEvent } from "../types/agent";
import type {
  ContextPreview,
  ModelAttempt,
  ModelUsage,
} from "../types/agentRun";
import type { MemorySelection } from "../types/agentMemory";

export interface ModelAttemptRecord {
  sequence?: number;
  purpose: string;
  attempt: ModelAttempt;
}

export interface TokenSubtotal {
  subtotal: number;
  unknownAttempts: number;
}

export interface ModelUsageSummary {
  attempts: ModelAttemptRecord[];
  completedAttempts: number;
  pendingAttempts: number;
  totals: Record<keyof ModelUsage, TokenSubtotal>;
}

export interface PreparedContext {
  preview: ContextPreview;
  memories: MemorySelection;
  willCompact?: boolean;
}

/** 按唯一 attempt ID 取最后状态，并统计已知用量与缺失项。 */
export function summarizeModelAttempts(
  records: readonly ModelAttemptRecord[],
): ModelUsageSummary {
  const ordered = records
    .map((record, index) => ({ record, index }))
    .sort(
      (left, right) =>
        (left.record.sequence ?? left.index) -
        (right.record.sequence ?? right.index),
    );
  const unique = new Map<string, ModelAttemptRecord>();
  for (const { record } of ordered) unique.set(record.attempt.id, record);
  const attempts = [...unique.values()];
  const terminal = attempts.filter(
    (record) => record.attempt.phase !== "started",
  );
  const fields: (keyof ModelUsage)[] = [
    "inputTokens",
    "outputTokens",
    "cachedInputTokens",
    "cacheCreationInputTokens",
    "reasoningTokens",
  ];
  const totals = Object.fromEntries(
    fields.map((field) => {
      let subtotal = 0;
      let unknownAttempts = 0;
      for (const record of terminal) {
        const value = record.attempt.usage?.[field];
        if (
          typeof value !== "number" ||
          !Number.isSafeInteger(value) ||
          value < 0 ||
          subtotal > Number.MAX_SAFE_INTEGER - value
        ) {
          unknownAttempts += 1;
          continue;
        }
        subtotal += value;
      }
      return [field, { subtotal, unknownAttempts }];
    }),
  ) as Record<keyof ModelUsage, TokenSubtotal>;
  return {
    attempts,
    completedAttempts: terminal.length,
    pendingAttempts: attempts.length - terminal.length,
    totals,
  };
}

/** 格式化用量字段的小计，并明示因缺失而未知的请求数。 */
export function formatTokenSubtotal(value: TokenSubtotal): string {
  const subtotal = value.subtotal.toLocaleString("zh-CN");
  return value.unknownAttempts > 0
    ? subtotal + " 已知小计 · " + value.unknownAttempts + " 次未知"
    : subtotal;
}

/** 格式化单项计数，拒绝将缺失、负值或不安全整数显示为真实用量。 */
export function formatTokenValue(value: number | null | undefined): string {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0
    ? value.toLocaleString("zh-CN")
    : "未知";
}

/** 将模型请求目的映射为清楚的中文标题。 */
export function modelAttemptPurposeLabel(purpose: string): string {
  switch (purpose) {
    case "reply":
      return "对话回复";
    case "compaction":
      return "上下文压缩";
    case "developmentSummary":
      return "开发总结";
    default:
      return "模型请求";
  }
}

/** 将请求终态映射为不暗示成功的中文标签。 */
export function modelAttemptPhaseLabel(phase: ModelAttempt["phase"]): string {
  switch (phase) {
    case "started":
      return "进行中";
    case "completed":
      return "已完成";
    case "failed":
      return "失败";
  }
}

/** 读取聊天事件中最新的已组装上下文和记忆选择事实。 */
export function latestPreparedContext(
  events: readonly AgentEvent[],
): PreparedContext | null {
  for (let index = events.length - 1; index >= 0; index--) {
    const event = events[index];
    if (event?.kind === "contextPrepared")
      return { preview: event.preview, memories: event.memories };
  }
  return null;
}
