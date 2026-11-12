/** 后端状态采用稳定英文枚举，界面只展示对应的中文事实。 */
export function aiJobStateLabel(state: string): string {
  return (
    (
      {
        running: "运行中",
        stopping: "正在停止",
        completed: "已结束",
        killed: "已停止",
        failed: "执行失败",
        interrupted: "历史任务未恢复",
      } as Record<string, string>
    )[state] ?? "状态未知"
  );
}
/** 退出码来自上游真实进程结果，保留其他原始诊断内容。 */
export function aiJobDetail(detail: string): string {
  return detail.replace(/^exit code:\s*(-?\d+)/, "退出码：$1");
}
