export interface SelectionAction {
  kind: "none" | "stage" | "unstage" | "mixed";
  label: string;
  reason: string;
}

/** 将两个索引方向的选择归为唯一动作，混合选择不能生成写请求。 */
export function selectionAction(
  stage: readonly string[],
  unstage: readonly string[],
): SelectionAction {
  if (stage.length && unstage.length)
    return {
      kind: "mixed",
      label: "暂存 / 取消暂存",
      reason: "不能同时处理暂存和取消暂存，请只保留一个方向的选择。",
    };
  if (stage.length)
    return { kind: "stage", label: `暂存所选 (${stage.length})`, reason: "" };
  if (unstage.length)
    return {
      kind: "unstage",
      label: `取消暂存 (${unstage.length})`,
      reason: "",
    };
  return { kind: "none", label: "暂存所选", reason: "请先选择文件。" };
}
