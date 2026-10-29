import { useState, type ChangeEvent } from "react";

/** 破坏性操作确认绑定当前一次性计划，换计划后旧勾选不能继续授权。 */
export function useDestructiveConfirmation(
  planId: string | undefined,
  required: boolean,
  execute: () => Promise<void>,
) {
  const [acceptedPlan, setAcceptedPlan] = useState<string | null>(null);
  const accepted = Boolean(planId && acceptedPlan === planId);
  /** 用户明确勾选不可恢复提示，只确认当前显示的计划。 */
  function change(event: ChangeEvent<HTMLInputElement>): void {
    setAcceptedPlan(event.target.checked && planId ? planId : null);
  }
  /** 程序调用处理器也检查当前确认，不依赖按钮 disabled 属性。 */
  async function confirm(): Promise<void> {
    if (required && !accepted) return;
    setAcceptedPlan(null);
    await execute();
  }
  return { accepted, change, confirm };
}
