export interface AiChatPreferences {
  transcriptView: "compact" | "standard" | "detailed" | "verbose";
  collapseTiming: "completion" | "next-input";
}
export type AiTurnState =
  "running" | "completed" | "failed" | "cancelled" | "interrupted";
/** 固定 DSH 展示矩阵；只有正常完成的整轮可以自动折叠。 */
export function aiProcessPolicy(
  preferences: AiChatPreferences,
  state: AiTurnState,
  hasNextInput: boolean,
) {
  const view = preferences.transcriptView;
  return {
    foldTurn:
      view !== "verbose" &&
      state === "completed" &&
      (preferences.collapseTiming === "completion" || hasNextInput),
    canFoldTurn: view !== "verbose" && state === "completed",
    stepGrouping:
      view === "verbose"
        ? ("none" as const)
        : view === "detailed"
          ? ("history" as const)
          : ("collapsed" as const),
    liveProcessDetail: view === "standard" || view === "detailed",
    settledReasoningPreview: view !== "compact",
  };
}
