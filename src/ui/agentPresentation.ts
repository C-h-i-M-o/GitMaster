import type { AgentEvent } from "../types/agent";
import type { PermissionMode } from "../types/agent";
import type { IconName } from "../components/Icon";
export {
  agentFinalMessage,
  formatTranscriptValue,
  projectAgentTranscript,
  providerThinking,
  transcriptRunStatus,
  transcriptToolStatus,
  transcriptToolSummary,
} from "./agentTranscript.ts";
export type {
  AgentTranscriptMessage,
  AgentTranscriptTool,
  AgentTranscriptTurn,
} from "./agentTranscript.ts";

const permissionIcons: Record<PermissionMode, IconName> = {
  requestApproval: "shield",
  autoApprove: "folderCheck",
  fullAccess: "shieldBolt",
};

/** 展示现有权限配置的名称、说明和图标，不计算或改变授权边界。 */
export function permissionPresentation(
  mode: PermissionMode,
  options: readonly {
    value: PermissionMode;
    label: string;
    description: string;
  }[],
): {
  label: string;
  description: string;
  icon: IconName;
} {
  const option = options.find((item) => item.value === mode);
  return {
    label: option?.label ?? mode,
    description: option?.description ?? "请在设置中检查此权限模式。",
    icon: permissionIcons[mode],
  };
}

/** 说明聊天管理按钮为何暂时禁用。 */
export function chatMenuDisabledReason(
  chatId: string,
  activeChatId: string | null,
  activeRunId: string | null,
  busy: boolean,
): string | null {
  if (busy) return "正在保存聊天操作，请稍候。";
  if (activeRunId !== null && chatId === activeChatId)
    return "此聊天正在执行；等待完成或停止后即可管理。";
  return null;
}

/** 为长文本生成有界预览，同时保留可展开的原始全文。 */
export function previewLongText(
  text: string,
  maximumLength = 1200,
): { preview: string; truncated: boolean; fullText: string } {
  const limit = Math.max(1, Math.floor(maximumLength));
  if (text.length <= limit)
    return { preview: text, truncated: false, fullText: text };
  let end = limit - 1;
  const previous = text.charCodeAt(end - 1);
  const next = text.charCodeAt(end);
  if (
    previous >= 0xd800 &&
    previous <= 0xdbff &&
    next >= 0xdc00 &&
    next <= 0xdfff
  )
    end--;
  return {
    preview: `${text.slice(0, end)}…`,
    truncated: true,
    fullText: text,
  };
}
/** 用真实字节数展示此设备用量，零与读取错误由调用方区分。 */
export function agentBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024)
    return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  return `${(bytes / 1024 / 1024 / 1024).toFixed(2)} GB`;
}
/** 事件展示保留工具参数和执行结果，协议原始推理字段不在界面展开。 */
export function eventPresentation(
  event: AgentEvent,
): { role: string; text: string; details?: string; kind: string } | null {
  switch (event.kind) {
    case "userMessage":
      return { role: "你", text: event.text, kind: "user" };
    case "assistantMessage":
      return event.text
        ? { role: "GitMaster", text: event.text, kind: "assistant" }
        : null;
    case "toolCall":
      return {
        role: "工具",
        text: event.call.name,
        details: JSON.stringify(event.call.arguments, null, 2),
        kind: "tool",
      };
    case "toolResult":
      if (
        event.name === "git_development_summary" &&
        !event.isError &&
        typeof event.result === "object" &&
        event.result !== null &&
        !Array.isArray(event.result) &&
        typeof event.result.report === "string"
      ) {
        return {
          role: "开发总结",
          text: event.result.report,
          kind: "assistant",
        };
      }
      return {
        role: event.isError ? "工具未完成" : "工具结果",
        text: event.name,
        details: JSON.stringify(event.result, null, 2),
        kind: event.isError ? "error" : "tool",
      };
    case "approvalDecided":
      return {
        role: "批准",
        text: event.approved ? "已批准本次操作" : "未批准本次操作",
        kind: "status",
      };
    case "summaryEvidence":
      return {
        role: "总结证据",
        text: "指定提交证据与同项目对话已保存",
        details: JSON.stringify(
          { evidence: event.evidence, relatedChats: event.relatedChats },
          null,
          2,
        ),
        kind: "tool",
      };
    case "runEnded":
      return {
        role: "本轮结束",
        text:
          event.message ??
          {
            completed: "已完成",
            cancelled: "已停止；已完成的操作不会回滚",
            failed: "运行失败",
            interrupted: "上次运行中断，未重放操作",
          }[event.status],
        kind: event.status === "failed" ? "error" : "status",
      };
    default:
      return null;
  }
}
/** 状态文字不把执行中误显示为成功。 */
export function runPhase(phase: string): string {
  return (
    {
      requesting: "正在生成…",
      compacting: "正在压缩历史上下文…",
      executingTool: "正在执行工具…",
      awaitingApproval: "等待你的批准",
      stopping: "正在停止；等待在途操作返回实际结果…",
    }[phase] ?? "正在处理…"
  );
}
