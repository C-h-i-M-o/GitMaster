import type { OperationResult } from "../types/git.ts";
import type { RepositoryViewState } from "../hooks/repositoryController.ts";

export type RefreshStatus =
  "idle" | "running" | "succeeded" | "failed" | "unverified" | "skipped";

/** 获取成功与本地刷新独立，失败刷新不能把网络成功改写为失败。 */
export function fetchStatus(result: OperationResult): RefreshStatus {
  if (result.outcome === "unknown") return "unverified";
  return result.outcome === "succeeded" ? "succeeded" : "failed";
}

/** 只对当前仓库的真实读取状态下结论，读取尚在排队时不能宣告成功。 */
export function localRefreshStatus(
  view: RepositoryViewState,
  repositoryId: string,
): RefreshStatus {
  if (view.repository?.repositoryId !== repositoryId) return "unverified";
  if (view.loading) return "running";
  return view.stale || view.error ? "failed" : "succeeded";
}

/** 两个阶段使用一致的简短状态文案。 */
export function refreshStatusLabel(status: RefreshStatus): string {
  return {
    idle: "未执行",
    running: "进行中",
    succeeded: "成功",
    failed: "失败",
    unverified: "尚未核实",
    skipped: "未执行",
  }[status];
}

/** 后端返回 Unix 毫秒文本，只格式化已核实成功时间，不以当前时间补值。 */
export function formatFetchedAt(value: string | null | undefined): string {
  if (value === undefined) return "记录暂不可用";
  if (value === null) return "本次会话尚无成功记录";
  if (!/^\d+$/.test(value)) return "获取时间不可用";
  const milliseconds = Number(value);
  if (
    !Number.isSafeInteger(milliseconds) ||
    milliseconds <= 0 ||
    milliseconds > 8.64e15
  )
    return "获取时间不可用";
  return new Date(milliseconds).toLocaleString("zh-CN", { hour12: false });
}
