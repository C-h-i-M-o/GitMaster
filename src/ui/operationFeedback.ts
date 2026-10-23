import type { Workbench } from "../hooks/useWorkbench";
import { describeGitError } from "./gitPresentation.ts";
import { describeOperationPhase } from "./operationPresentation.ts";

export interface OperationFeedbackModel {
  key: string;
  kind: "open" | "history" | "commit";
  compact: boolean;
  title: string;
  description: string;
  phase: string;
  error: boolean;
  action?: () => void | Promise<void>;
  actionLabel?: string;
}

/** 打开反馈位于抽屉上方，切换项目时也能看到等待与恢复入口。 */
export function openingFeedback(
  w: Pick<Workbench, "repo" | "canOpen" | "openRecent">,
): OperationFeedbackModel | null {
  const { repo } = w;
  if (repo.openingPath && (repo.loading || repo.error)) {
    const path = repo.openingPath;
    return {
      key: `open:${path}`,
      kind: "open",
      compact: Boolean(repo.repository),
      title: repo.error ? "暂时无法打开项目" : "正在打开项目",
      description: repo.error
        ? describeGitError(repo.error)
        : "读取仓库信息，准备你的工作区。",
      phase: repo.error ? "读取未完成" : "读取仓库信息",
      error: Boolean(repo.error),
      action: repo.error && w.canOpen ? w.openRecent(path) : undefined,
      actionLabel: "重试读取",
    };
  }
  return null;
}

/** 只派生真实读取状态，不用计时或动画推进任务。 */
export function graphFeedback(
  w: Pick<Workbench, "repo" | "history">,
): OperationFeedbackModel | null {
  const { repo, history } = w;
  if (!repo.repository || repo.openingPath) return null;
  const error =
    history.error && (history.stale || !history.page) ? history.error : null;
  if (!repo.loading && !history.loading && !history.loadingMore && !error)
    return null;
  return {
    key: `history:${repo.repository.repositoryId}:${history.loadingMore ? "more" : "root"}`,
    kind: "history",
    compact: Boolean(history.page),
    error: Boolean(error),
    title: error
      ? "历史暂未更新"
      : history.loadingMore
        ? "正在加载更多提交"
        : history.page
          ? "正在更新提交历史"
          : "正在读取提交历史",
    description: error
      ? describeGitError(error)
      : history.page
        ? "保留当前视图，读取最新的提交与分支。"
        : "读取提交与分支，准备历史视图。",
    phase: error
      ? "读取未完成"
      : repo.loading
        ? "读取仓库状态"
        : "读取提交历史",
    action: error ? history.refresh : undefined,
    actionLabel: "重试读取",
  };
}

/** 提交只使用当前项目的任务身份；未知结果仅允许恢复查询。 */
export function commitFeedback(
  w: Pick<Workbench, "repo" | "operations">,
): OperationFeedbackModel | null {
  const { operations: op, repo } = w;
  if (
    op.operationScope?.kind !== "commit" ||
    op.operationScope.repositoryId !== repo.repository?.repositoryId ||
    op.activity === "idle"
  )
    return null;
  const error = op.activity === "unverified";
  return {
    key: `commit:${repo.repository?.repositoryId}`,
    kind: "commit",
    compact: true,
    error,
    title: error
      ? "提交结果尚未确认"
      : op.activity === "preparing"
        ? "正在准备提交"
        : op.activity === "querying"
          ? "正在核对本地状态"
          : "正在创建提交",
    description: error
      ? "先查询任务结果，请勿重复提交。"
      : "正在保存本次版本，请勿重复提交。",
    phase: error
      ? "需要核对"
      : op.activity === "preparing"
        ? "检查提交范围"
        : op.record && !op.record.result
          ? describeOperationPhase(op.record.progress.phase)
          : "等待 Git 执行结果",
    action: error ? op.resume : undefined,
    actionLabel: "核对本地状态",
  };
}
