import type { OperationError } from "../types/git.ts";
import { describeGitError } from "./gitPresentation.ts";

export interface WorkspaceNotice {
  id: string;
  tone: "progress" | "success" | "warning" | "error";
  message: string;
  detail?: string;
  action?: "remote" | "resume";
}
export interface WorkspaceNoticeInput {
  sync: {
    busy: boolean;
    phaseLabel: string;
    notice: string | null;
    diverged: boolean;
    error: OperationError | null;
  };
  refresh: {
    busy: boolean;
    notice: string | null;
    resultLabel: string | null;
    error: OperationError | null;
    networkStatus: string;
    localStatus: string;
  };
  errors: readonly (OperationError | null)[];
  stale: boolean;
  activity: string;
}
/** 错误优先且按中文原因去重；成功摘要不会覆盖失败或未核实状态。 */
export function workspaceNotices(
  input: WorkspaceNoticeInput,
): WorkspaceNotice[] {
  const result: WorkspaceNotice[] = [];
  const messages = new Set<string>();
  for (const error of [
    ...input.errors,
    input.sync.error,
    input.refresh.error,
  ]) {
    if (!error) continue;
    const message = describeGitError(error);
    if (messages.has(message)) continue;
    messages.add(message);
    result.push({
      id: `error:${error.code}:${message}`,
      tone: "error",
      message,
    });
  }
  if (input.stale)
    result.push({
      id: "stale",
      tone: "warning",
      message: "正在展示旧状态",
      detail: "仓库刷新失败，请刷新后继续写操作。",
    });
  if (input.activity === "unverified")
    result.push({
      id: "unverified",
      tone: "warning",
      message: "任务状态尚未核实",
      action: "resume",
    });
  if (input.sync.busy)
    result.push({
      id: "sync-progress",
      tone: "progress",
      message: input.sync.phaseLabel,
    });
  else if (input.sync.notice)
    result.push({
      id: `sync:${input.sync.notice}`,
      tone: input.sync.error || input.sync.diverged ? "warning" : "success",
      message: input.sync.notice,
      action: input.sync.diverged ? "remote" : undefined,
    });
  if (input.refresh.busy)
    result.push({
      id: "refresh-progress",
      tone: "progress",
      message: "正在获取远端并刷新本地…",
    });
  else if (input.refresh.resultLabel || input.refresh.notice) {
    const failed =
      Boolean(input.refresh.error) ||
      [input.refresh.networkStatus, input.refresh.localStatus].some(
        (value) => value === "failed" || value === "unverified",
      );
    const running = [
      input.refresh.networkStatus,
      input.refresh.localStatus,
    ].includes("running");
    result.push({
      id: `refresh:${input.refresh.resultLabel}:${input.refresh.notice}`,
      tone: failed ? "warning" : running ? "progress" : "success",
      message: failed
        ? "刷新未全部完成"
        : running
          ? "正在刷新本地…"
          : "刷新完成",
      detail: [input.refresh.notice, input.refresh.resultLabel]
        .filter(Boolean)
        .join(" "),
    });
  }
  if (input.activity === "preparing" && !input.sync.busy && !input.refresh.busy)
    result.push({
      id: "preparing",
      tone: "progress",
      message: "正在准备操作…",
    });
  return result;
}
