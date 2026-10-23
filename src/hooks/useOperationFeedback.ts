import { useEffect, useState } from "react";
import type { OperationFeedbackModel } from "../ui/operationFeedback";

/** 等待计时不推进业务阶段；错误时停止，任务变更和卸载时清理。 */
export function useOperationFeedback(model: OperationFeedbackModel) {
  const [clock, setClock] = useState({
    key: model.key,
    started: Date.now(),
    now: Date.now(),
  });
  useEffect(() => {
    if (model.error) return;
    const now = Date.now();
    setClock({ key: model.key, started: now, now });
  }, [model.key, model.error]);
  useEffect(() => {
    if (model.error) return;
    const timer = window.setInterval(
      () => setClock((old) => ({ ...old, now: Date.now() })),
      1000,
    );
    return () => window.clearInterval(timer);
  }, [model.key, model.error]);
  const seconds =
    clock.key === model.key
      ? Math.max(0, Math.floor((clock.now - clock.started) / 1000))
      : 0;
  return {
    duration: `${String(Math.floor(seconds / 60)).padStart(2, "0")}:${String(seconds % 60).padStart(2, "0")}`,
    long: !model.error && seconds >= 12,
    longTip:
      model.kind === "commit"
        ? "Git 尚未返回结果，请勿重复提交。"
        : "等待比平时稍久，仍在等待 Git 返回。无需重复打开项目。",
  };
}
