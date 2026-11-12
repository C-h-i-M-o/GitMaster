import type { AiRow } from "../hooks/useAiWorkspace";
import {
  aiProcessPolicy,
  type AiChatPreferences,
  type AiTurnState,
} from "./aiProcessPolicy.ts";
/** 聚合标题仅展示有界活动详情，不在简洁模式暴露完整输入。 */
export function aiLiveDetail(rows: AiRow[]): string {
  const row = [...rows]
    .reverse()
    .find((item) => item.event.kind === "tool" && item.event.output === null);
  return row?.event.kind === "tool" ? row.event.input.slice(0, 300) : "";
}

/** 按上游原始轮次分组，旧无来源事件保持可见而不推测其归属。 */
export function buildAiTurns(
  rows: AiRow[],
  preferences: AiChatPreferences,
  busy: boolean,
) {
  const groups = new Map<number, AiRow[]>();
  for (const row of rows) {
    const turn = row.turn ?? (row.event.kind === "turn" ? row.event.turn : -1);
    const list = groups.get(turn) ?? [];
    list.push(row);
    groups.set(turn, list);
  }
  const entries = [...groups.entries()];
  return entries.map(([turn, items], index) => {
    const lifecycle = [...items]
      .reverse()
      .find((row) => row.event.kind === "turn")?.event;
    const state: AiTurnState =
      lifecycle?.kind === "turn"
        ? lifecycle.state
        : busy && index === entries.length - 1
          ? "running"
          : "interrupted";
    const hasNextInput = entries
      .slice(index + 1)
      .some(([, next]) => next.some((row) => row.event.kind === "user"));
    const policy = aiProcessPolicy(preferences, state, hasNextInput);
    const process = items
      .filter(
        (row) =>
          row.event.kind === "tool" ||
          (row.event.kind === "assistant" && Boolean(row.event.reasoning)),
      )
      .map((row) => {
        if (
          row.event.kind !== "tool" ||
          row.event.output !== null ||
          state === "running"
        )
          return row;
        return {
          ...row,
          event: {
            ...row.event,
            output:
              state === "cancelled"
                ? "工具已取消，未收到完成结果。"
                : "工具已中断，未收到完成结果。",
            failed: true,
            state:
              state === "cancelled"
                ? ("cancelled" as const)
                : ("interrupted" as const),
          },
        };
      });
    const steps = new Map<number, AiRow[]>();
    for (const row of process) {
      const step = row.step ?? -1;
      const list = steps.get(step) ?? [];
      list.push(row);
      steps.set(step, list);
    }
    return {
      turn,
      state,
      policy,
      process,
      steps: [...steps.entries()],
      messages: items.filter(
        (row) => row.event.kind !== "tool" && row.event.kind !== "turn",
      ),
      resetVersion: `${preferences.transcriptView}-${preferences.collapseTiming}-${policy.foldTurn}`,
    };
  });
}
