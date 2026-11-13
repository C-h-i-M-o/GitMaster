import type {
  AiEventEnvelope,
  AiFailure,
} from "../../services/agent-host/src/contracts.ts";
import type { AiTurnState } from "./aiProcessPolicy.ts";

export type AiHistoryState = "idle" | "loading" | "ready" | "failed";
export interface AiIssue extends AiFailure {
  scope: "history" | "runtime" | "model-request" | "job";
  title: string;
  sessionId: string | null;
  runtimeEpoch: string | null;
  runId: string | null;
  turn: number | null;
}
export interface AiConnectionCheck {
  state: "idle" | "testing" | "succeeded" | "failed";
  key: string;
  checkedAt: number | null;
  error: AiFailure | null;
}

/** 合并分页与实时事件，只接纳当前会话和运行时代次，保持既有内存上限。 */
export function mergeAiHistory(
  history: readonly AiEventEnvelope[],
  live: readonly AiEventEnvelope[],
  sessionId: string,
  runtimeEpoch: string,
): AiEventEnvelope[] {
  if (typeof runtimeEpoch !== "string" || !runtimeEpoch)
    throw { code: "AI_PROTOCOL" };
  const rows = new Map<number, AiEventEnvelope>();
  for (const row of [...history, ...live]) {
    if (row.sessionId === sessionId && row.runtimeEpoch === runtimeEpoch)
      rows.set(row.sequence, row);
  }
  return [...rows.values()]
    .sort((a, b) => a.sequence - b.sequence)
    .slice(-6000);
}

/** 只将最新轮次的错误投影到恢复区域，较早失败始终保留在消息时间线。 */
export function aiRequestIssue(
  events: readonly AiEventEnvelope[],
  activeRunId: string | null,
): AiIssue | null {
  let turn: number | null = null;
  let state: AiTurnState | null = null;
  let failure: AiEventEnvelope | null = null;
  for (const row of events) {
    const position = row.event.kind === "turn" ? row.event.turn : row.turn;
    if (
      position !== undefined &&
      position !== null &&
      (turn === null || position > turn)
    ) {
      turn = position;
      failure = null;
      state = null;
    }
    if (position !== undefined && position !== null && position !== turn)
      continue;
    if (row.event.kind === "error" && turn !== null) failure = row;
    if (row.event.kind === "turn") state = row.event.state;
  }
  if (
    !failure ||
    failure.event.kind !== "error" ||
    (state !== null && state !== "failed" && state !== "running")
  )
    return null;
  return {
    scope: "model-request",
    title:
      activeRunId && failure.runId === activeRunId
        ? "本次请求失败"
        : "上一轮请求失败",
    code: failure.event.code,
    message: failure.event.message,
    sessionId: failure.sessionId,
    runtimeEpoch: failure.runtimeEpoch,
    runId: failure.runId,
    turn,
  };
}

/** 使用稳定错误码选择恢复动作，未知请求失败不猜测为网络问题。 */
export function aiRecoveryAction(
  issue: AiIssue,
): "reload" | "connect" | "settings" | "jobs" | null {
  if (
    [
      "AI_BUSY",
      "AI_CREDENTIAL_BUSY",
      "AI_UNSAVED_FILES",
      "AI_DECISION_EXPIRED",
      "AI_JOBS_RUNNING",
      "AI_PROTOCOL",
    ].includes(issue.code)
  )
    return null;
  if (issue.code === "AI_UNKNOWN_MODEL") return "settings";
  if (
    /^(AI_RUNTIME_(STOPPED|STOPPING|START|FAILED|NOT_STARTED)|AI_TIMEOUT)$/.test(
      issue.code,
    )
  )
    return "reload";
  if (issue.scope === "job") return "jobs";
  if (issue.scope === "history") return "reload";
  if (issue.scope === "runtime") return null;
  if (
    /^(AI_NETWORK_ERROR|AI_TLS_CERTIFICATE|TIMEOUT|TRANSPORT|STREAM_CLOSED|AI_CONNECTION_FAILED)$/.test(
      issue.code,
    )
  )
    return "connect";
  if (
    /^(AI_HTTP_(400|401|402|403|404|413|422)|AUTH|QUOTA_EXCEEDED|MISSING_CREDENTIAL|INVALID_CREDENTIAL|INVALID_REQUEST|AI_UNKNOWN_MODEL)$/.test(
      issue.code,
    )
  )
    return "settings";
  return null;
}

/** 从 RPC 错误中读取有限稳定码，不传递原始异常或服务商正文。 */
export function aiFailureCode(cause: unknown): string {
  return cause &&
    typeof cause === "object" &&
    "code" in cause &&
    typeof cause.code === "string" &&
    /^[A-Z_0-9]{2,64}$/.test(cause.code)
    ? cause.code
    : "AI_RUNTIME_FAILED";
}
