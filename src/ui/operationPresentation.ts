import type { OperationKind, OperationPhase } from "../types/git";
import type { RecoveryPathResult } from "../types/git";

/** 逐文件结果只描述已经核实的状态，不把未执行项显示为已回滚。 */
export function describeRecoveryPathStatus(
  status: RecoveryPathResult["status"],
): string {
  return {
    succeeded: "已完成",
    failed: "失败",
    notExecuted: "未执行",
    unknown: "结果未知",
  }[status];
}

/** 将后端毫秒时间戳格式化为本地可读时间。 */
export function formatOperationExpiry(timestamp: number): string {
  return new Date(timestamp).toLocaleString();
}

/** 将后端操作类型转换为界面中文名称。 */
export function describeOperationKind(kind: OperationKind): string {
  return (
    {
      restoreFiles: "恢复文件",
      cleanUntracked: "清理未跟踪文件",
      stage: "暂存",
      unstage: "取消暂存",
      commit: "提交",
      createBranch: "创建分支",
      switchBranch: "切换分支",
      clone: "克隆仓库",
      fetch: "获取远端",
      push: "推送",
      integrate: "整合远端",
      saveConflict: "保存冲突解决",
      saveRevertConflict: "保存撤销冲突解决",
      revert: "应用撤销修改",
      finishMerge: "完成合并",
      finishRevert: "完成撤销",
      abortRevert: "中止撤销",
      saveFile: "保存项目文件",
      setUpstream: "设置当前分支上游",
    } satisfies Record<OperationKind, string>
  )[kind];
}

/** 将后端操作阶段转换为界面中文名称。 */
export function describeOperationPhase(phase: OperationPhase): string {
  return (
    {
      queued: "排队中",
      checking: "检查中",
      transferring: "传输中",
      writing: "写入中",
      verifying: "校验中",
      completed: "已完成",
      failed: "失败",
      needsResolution: "需要处理冲突",
      unknown: "结果未知",
    } satisfies Record<OperationPhase, string>
  )[phase];
}
