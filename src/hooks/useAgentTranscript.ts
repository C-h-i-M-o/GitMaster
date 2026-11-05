import { useMemo } from "react";
import type { AgentEvent } from "../types/agent";
import { projectAgentTranscript } from "../ui/agentTranscript";

/** 按持久事件变更缓存对话投影，避免流式状态更新重新解析历史。 */
export function useAgentTranscript(
  events: readonly AgentEvent[],
  activeRunId: string | null,
) {
  const turns = useMemo(() => projectAgentTranscript(events), [events]);
  const activeTurn = turns.find((turn) => turn.runId === activeRunId) ?? null;
  return { turns, activeTurn };
}
