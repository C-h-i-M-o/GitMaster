import type {
  OperationRecord,
  OperationError,
  OperationKind,
  OperationPhase,
  OperationCounts,
} from "../types/git.ts";

export interface HistoryEvent {
  sequence: number;
  phase: OperationPhase;
  counts: OperationCounts | null;
  observedAt: number;
}
export interface OperationHistoryEntry {
  id: string;
  repositoryId: string | null;
  target: string;
  kind: OperationKind;
  sequence: number;
  phase: OperationPhase;
  startedAt: number;
  finishedAt: number | null;
  outcome: "succeeded" | "failed" | "unknown" | "needsResolution" | null;
  error: OperationError | null;
  refreshError: OperationError | null;
  commitOid: string | null;
  unresolvedCount: number | null;
  recoveryPath: string | null;
  needsMergeCommit: boolean;
  events: HistoryEvent[];
}

let preparationSequence = 0;

/** 将前置校验失败记录为前端摘要，不伪造后端任务结果。 */
export function appendPreparationFailure(
  history: readonly OperationHistoryEntry[],
  kind: OperationKind,
  repositoryId: string | null,
  target: string,
  error: OperationError,
  now: number,
): OperationHistoryEntry[] {
  if (kind === "saveFile" || kind === "saveConflict") return [...history];
  const id = `prepare-${now}-${++preparationSequence}`;
  return [
    {
      id,
      repositoryId,
      target,
      kind,
      sequence: 0,
      phase: "failed" as const,
      startedAt: now,
      finishedAt: now,
      outcome: "failed" as const,
      error,
      refreshError: null,
      commitOid: null,
      unresolvedCount: null,
      recoveryPath: null,
      needsMergeCommit: false,
      events: [],
    },
    ...history,
  ].slice(0, 200);
}

/** 按任务聚合有界摘要，拒绝倒序/跨仓库响应，不保留完整仓库快照。 */
export function updateOperationHistory(
  history: readonly OperationHistoryEntry[],
  record: OperationRecord,
  target: string,
  now: number,
): OperationHistoryEntry[] {
  const p = record.progress,
    result = record.result;
  if (p.kind === "saveFile" || p.kind === "saveConflict") return [...history];
  const previous = history.find((item) => item.id === p.handle.operationId);
  if (
    previous &&
    (previous.repositoryId !== p.handle.repositoryId ||
      previous.sequence > p.sequence ||
      (previous.outcome !== null && !result))
  )
    return [...history];
  const event: HistoryEvent = {
    sequence: p.sequence,
    phase: p.phase,
    counts: p.counts,
    observedAt: now,
  };
  const events = previous?.events ?? [];
  const duplicate =
    events.at(-1)?.sequence === p.sequence && events.at(-1)?.phase === p.phase;
  const entry: OperationHistoryEntry = {
    id: p.handle.operationId,
    repositoryId: p.handle.repositoryId,
    target: previous?.target || target,
    kind: p.kind,
    sequence: p.sequence,
    phase: p.phase,
    startedAt: p.startedAt,
    finishedAt: result ? (previous?.finishedAt ?? now) : null,
    outcome: result?.outcome ?? null,
    error:
      result?.outcome === "failed" || result?.outcome === "unknown"
        ? result.error
        : null,
    refreshError:
      result?.refresh.status === "failed" ? result.refresh.error : null,
    commitOid: result?.outcome === "succeeded" ? result.commitOid : null,
    unresolvedCount:
      result?.outcome === "needsResolution"
        ? result.conflict.unresolvedCount
        : null,
    recoveryPath:
      result?.outcome === "failed" || result?.outcome === "unknown"
        ? (result.cloneRecovery?.path ?? null)
        : null,
    events: duplicate ? events : [...events, event].slice(-100),
    needsMergeCommit:
      result?.outcome === "succeeded" &&
      result.kind === "integrate" &&
      result.commitOid === null &&
      result.refresh.status === "ready" &&
      result.refresh.state.operations.includes("merge"),
  };
  const all = [...history.filter((item) => item.id !== entry.id), entry];
  const active = all.filter((item) => item.finishedAt === null);
  const ended = all
    .filter((item) => item.finishedAt !== null)
    .sort((a, b) => (b.finishedAt ?? 0) - (a.finishedAt ?? 0))
    .slice(0, 200);
  return [...active.sort((a, b) => b.startedAt - a.startedAt), ...ended];
}
