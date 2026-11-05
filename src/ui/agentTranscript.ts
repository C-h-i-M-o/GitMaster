import type {
  AgentEvent,
  ApprovalRequest,
  JsonValue,
  PermissionMode,
  ProviderProtocol,
  ToolCall,
} from "../types/agent";
import type { AgentFileChange, ModelAttempt } from "../types/agentRun";
import type { ModelParameters } from "../types/agentModels";
import type { VerifiedReference } from "../types/agentContext";

/** 一条已配对的工具操作及其审批、结果和证据。 */
export interface AgentTranscriptTool {
  callId: string;
  name: string;
  arguments: JsonValue | null;
  result: JsonValue | null;
  completed: boolean;
  isError: boolean;
  approval: ApprovalRequest | null;
  approved: boolean | null;
  evidence: { evidence: JsonValue; relatedChats: string[] }[];
}

/** 一条实际由供应商返回的助手消息。 */
export interface AgentTranscriptMessage {
  sequence: number;
  text: string;
  calls: ToolCall[];
  thinking: string[];
}

/** 一次写入事件引用的不可变历史快照。 */
export interface AgentTranscriptChange {
  sequence: number;
  callId: string;
  change: AgentFileChange;
}

/** 不阻断运行的后端提示，保留事件顺序。 */
export interface AgentTranscriptWarning {
  sequence: number;
  message: string;
}

/** 本轮实际生效的参数与模型、连接修订快照。 */
export interface AgentTranscriptConfiguration {
  sequence: number;
  parameters: ModelParameters;
  modelRevision: number | null;
  connectionRevision: number | null;
}

/** 一次模型请求及其所属用途、持久事件顺序。 */
export interface AgentTranscriptModelAttempt {
  sequence: number;
  purpose: string;
  attempt: ModelAttempt;
}

/** 一条由模型建议、尚未保存的工作区记忆。 */
export interface AgentTranscriptMemorySuggestion {
  sequence: number;
  text: string;
  tags: string[];
}

/** 本轮用户明确提供的历史资料引用。 */
export interface AgentTranscriptReferences {
  sequence: number;
  references: VerifiedReference[];
}

/** 按运行组织的持久对话投影，不包含原始协议字段。 */
export interface AgentTranscriptTurn {
  sessionId: string;
  runId: string;
  firstSequence: number;
  userMessage: string | null;
  userSequence: number | null;
  assistantMessages: AgentTranscriptMessage[];
  processMessages: AgentTranscriptMessage[];
  finalMessage: AgentTranscriptMessage | null;
  tools: AgentTranscriptTool[];
  changes: AgentTranscriptChange[];
  warnings: AgentTranscriptWarning[];
  configurations: AgentTranscriptConfiguration[];
  modelAttempts: AgentTranscriptModelAttempt[];
  memorySuggestions: AgentTranscriptMemorySuggestion[];
  references: AgentTranscriptReferences[];
  permissionMode: PermissionMode | null;
  status: "running" | "completed" | "cancelled" | "failed" | "interrupted";
  ended: boolean;
  errorMessage: string | null;
  hasError: boolean;
  hasPendingApproval: boolean;
  hasAttention: boolean;
  hasProcess: boolean;
}

interface MutableTurn {
  sessionId: string;
  runId: string;
  firstSequence: number;
  userMessage: string | null;
  userSequence: number | null;
  assistantMessages: AgentTranscriptMessage[];
  tools: Map<string, AgentTranscriptTool>;
  changes: AgentTranscriptChange[];
  warnings: AgentTranscriptWarning[];
  configurations: AgentTranscriptConfiguration[];
  modelAttempts: AgentTranscriptModelAttempt[];
  memorySuggestions: AgentTranscriptMemorySuggestion[];
  references: AgentTranscriptReferences[];
  permissionMode: PermissionMode | null;
  status: AgentTranscriptTurn["status"];
  ended: boolean;
  errorMessage: string | null;
  approvalTools: Map<string, AgentTranscriptTool>;
  approvalDecisions: Map<string, boolean>;
}

interface TranscriptCall {
  id: string;
  name: string;
  arguments: JsonValue | null;
}

/** 读取带目标类型的对象值，拒绝数组和 null。 */
function objectValue(value: JsonValue): Record<string, JsonValue> | null {
  if (typeof value !== "object" || value === null || Array.isArray(value))
    return null;
  return value;
}

/** 仅把协议中明示的可展示思考摘要纳入界面。 */
export function providerThinking(
  protocol: ProviderProtocol,
  wire: JsonValue,
): string[] {
  const root = objectValue(wire);
  if (!root) return [];
  if (protocol === "chatCompletions") {
    const text = root.reasoning_content;
    return typeof text === "string" && text.length > 0 ? [text] : [];
  }
  if (protocol === "anthropicMessages" && Array.isArray(root.content)) {
    return root.content.flatMap((part) => {
      const block = objectValue(part);
      return block?.type === "thinking" &&
        typeof block.thinking === "string" &&
        block.thinking.length > 0
        ? [block.thinking]
        : [];
    });
  }
  if (protocol !== "responses" || !Array.isArray(root.output)) return [];
  const summaries: string[] = [];
  for (const item of root.output) {
    const reasoning = objectValue(item);
    if (reasoning?.type !== "reasoning" || !Array.isArray(reasoning.summary))
      continue;
    for (const part of reasoning.summary) {
      const summary = objectValue(part);
      const text = summary?.text;
      if (
        summary?.type === "summary_text" &&
        typeof text === "string" &&
        text.length > 0
      )
        summaries.push(text);
    }
  }
  return summaries;
}

/** 汇总重复事件并将调用、审批和结果按聊天、运行及 callId 配对。 */
export function projectAgentTranscript(
  events: readonly AgentEvent[],
): AgentTranscriptTurn[] {
  const uniqueEvents = new Map<string, AgentEvent>();
  for (const event of events)
    uniqueEvents.set(JSON.stringify([event.sessionId, event.sequence]), event);
  const ordered = [...uniqueEvents.values()].sort(
    (left, right) =>
      left.sessionId.localeCompare(right.sessionId) ||
      left.sequence - right.sequence,
  );
  const turns = new Map<string, MutableTurn>();
  for (const event of ordered) {
    const turnKey = JSON.stringify([event.sessionId, event.runId]);
    let turn = turns.get(turnKey);
    if (!turn) {
      turn = {
        sessionId: event.sessionId,
        runId: event.runId,
        firstSequence: event.sequence,
        userMessage: null,
        userSequence: null,
        assistantMessages: [],
        tools: new Map(),
        changes: [],
        warnings: [],
        configurations: [],
        modelAttempts: [],
        memorySuggestions: [],
        references: [],
        permissionMode: null,
        status: "running",
        ended: false,
        errorMessage: null,
        approvalTools: new Map(),
        approvalDecisions: new Map(),
      };
      turns.set(turnKey, turn);
    }
    turn.firstSequence = Math.min(turn.firstSequence, event.sequence);
    switch (event.kind) {
      case "runStarted":
        turn.permissionMode = event.permissionMode;
        break;
      case "fileChanged":
        turn.changes.push({
          sequence: event.sequence,
          callId: event.callId,
          change: event.change,
        });
        break;
      case "runWarning":
        turn.warnings.push({
          sequence: event.sequence,
          message: event.message,
        });
        break;
      case "runConfiguration":
        turn.configurations.push({
          sequence: event.sequence,
          parameters: event.parameters,
          modelRevision: event.modelRevision,
          connectionRevision: event.connectionRevision,
        });
        break;
      case "modelAttempt":
        turn.modelAttempts.push({
          sequence: event.sequence,
          purpose: event.purpose,
          attempt: event.attempt,
        });
        break;
      case "memorySuggested":
        turn.memorySuggestions.push({
          sequence: event.sequence,
          text: event.text,
          tags: event.tags,
        });
        break;
      case "userReferences":
        turn.references.push({
          sequence: event.sequence,
          references: event.references,
        });
        break;
      case "userMessage":
        turn.userMessage = event.text;
        turn.userSequence = event.sequence;
        break;
      case "assistantMessage": {
        const thinking = providerThinking(event.protocol, event.wire);
        turn.assistantMessages.push({
          sequence: event.sequence,
          text: event.text,
          calls: event.calls,
          thinking,
        });
        for (const call of event.calls) mergeCall(turn, call);
        break;
      }
      case "toolCall":
        mergeCall(turn, event.call);
        break;
      case "approvalRequested": {
        const tool = mergeCall(turn, event.approval.call);
        tool.approval = event.approval;
        tool.approved = turn.approvalDecisions.get(event.approval.id) ?? null;
        turn.approvalTools.set(event.approval.id, tool);
        break;
      }
      case "approvalDecided": {
        turn.approvalDecisions.set(event.approvalId, event.approved);
        const tool = turn.approvalTools.get(event.approvalId);
        if (tool) tool.approved = event.approved;
        break;
      }
      case "toolResult": {
        const tool = mergeCall(turn, {
          id: event.callId,
          name: event.name,
          arguments: null,
        });
        tool.result = event.result;
        tool.isError = event.isError;
        tool.completed = true;
        break;
      }
      case "summaryEvidence": {
        const tool = mergeCall(turn, {
          id: event.callId,
          name: "总结证据",
          arguments: null,
        });
        tool.evidence.push({
          evidence: event.evidence,
          relatedChats: event.relatedChats,
        });
        break;
      }
      case "runEnded":
        turn.status = event.status;
        turn.ended = true;
        turn.errorMessage = event.status === "failed" ? event.message : null;
        break;
    }
  }
  return [...turns.values()]
    .sort((left, right) => left.firstSequence - right.firstSequence)
    .map((turn) => {
      const tools = [...turn.tools.values()];
      const finalMessage = lastFinalMessage(turn.assistantMessages);
      const processMessages = turn.assistantMessages.filter(
        (message) => message !== finalMessage,
      );
      const hasError =
        turn.status === "failed" || tools.some((tool) => tool.isError);
      const hasPendingApproval = tools.some(
        (tool) => tool.approval !== null && tool.approved === null,
      );
      return {
        sessionId: turn.sessionId,
        runId: turn.runId,
        firstSequence: turn.firstSequence,
        userMessage: turn.userMessage,
        userSequence: turn.userSequence,
        assistantMessages: turn.assistantMessages,
        processMessages,
        finalMessage,
        tools,
        changes: turn.changes,
        warnings: turn.warnings,
        configurations: turn.configurations,
        modelAttempts: turn.modelAttempts,
        memorySuggestions: turn.memorySuggestions,
        references: turn.references,
        permissionMode: turn.permissionMode,
        status: turn.status,
        ended: turn.ended,
        errorMessage: turn.errorMessage,
        hasError,
        hasPendingApproval,
        hasAttention: hasError || hasPendingApproval,
        hasProcess:
          processMessages.length > 0 ||
          tools.length > 0 ||
          turn.changes.length > 0 ||
          turn.configurations.length > 0 ||
          turn.modelAttempts.length > 0 ||
          turn.memorySuggestions.length > 0 ||
          turn.references.length > 0,
      };
    });
}

/** 合并相同调用的模型意图与后端执行事件。 */
function mergeCall(
  turn: MutableTurn,
  call: TranscriptCall,
): AgentTranscriptTool {
  const existing = turn.tools.get(call.id);
  if (existing) {
    if (call.arguments !== null) existing.arguments = call.arguments;
    if (call.name && (!existing.name || existing.name === "总结证据"))
      existing.name = call.name;
    return existing;
  }
  const tool: AgentTranscriptTool = {
    callId: call.id,
    name: call.name,
    arguments: call.arguments,
    result: null,
    completed: false,
    isError: false,
    approval: null,
    approved: null,
    evidence: [],
  };
  turn.tools.set(call.id, tool);
  return tool;
}

/** 选取本轮最后一条不含工具调用的助手回复作为最终回答。 */
export function agentFinalMessage(
  turn: AgentTranscriptTurn,
): AgentTranscriptMessage | null {
  return turn.finalMessage;
}

/** 从消息列表中选取最后一条不含工具调用的回复。 */
function lastFinalMessage(
  messages: AgentTranscriptMessage[],
): AgentTranscriptMessage | null {
  for (let index = messages.length - 1; index >= 0; index--) {
    const message = messages[index];
    if (message && message.calls.length === 0) return message;
  }
  return null;
}

/** 为运行头部生成不暗示成功的状态文字。 */
export function transcriptRunStatus(turn: AgentTranscriptTurn): string {
  return (
    {
      running: "处理中",
      completed: "已完成",
      cancelled: "已停止",
      failed: "运行失败",
      interrupted: "上次运行中断",
    } as const
  )[turn.status];
}

/** 为配对后的工具卡生成真实执行状态。 */
export function transcriptToolStatus(tool: AgentTranscriptTool): string {
  if (tool.approval && tool.approved === null) return "等待批准";
  if (tool.approval && tool.approved === false) return "未批准";
  if (tool.isError) return "执行失败";
  if (tool.completed) return "已完成";
  if (tool.approval?.id && tool.approved === true) return "已批准";
  return "等待结果";
}

/** 格式化安全 JSON 值供纯文本节点展示。 */
export function formatTranscriptValue(value: JsonValue | null): string {
  return JSON.stringify(value, null, 2) ?? "";
}

/** 概括工具成功和失败数，不计算或推断工具副作用。 */
export function transcriptToolSummary(turn: AgentTranscriptTurn): string {
  if (turn.tools.length === 0)
    return turn.changes.length > 0
      ? `${turn.changes.length} 项文件变更`
      : "没有工具操作";
  const failed = turn.tools.filter((tool) => tool.isError).length;
  const completed = turn.tools.filter(
    (tool) => tool.completed && !tool.isError,
  ).length;
  const changes =
    turn.changes.length > 0 ? ` · ${turn.changes.length} 项文件变更` : "";
  return `${turn.tools.length} 项工具操作 · ${completed} 成功 / ${failed} 失败${changes}`;
}
