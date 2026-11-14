import type { AiModelRef } from "../types/ai";

/** 保留已有选择，仅为尚未选择模型的新会话提供可用模型。 */
export function chooseAiModel(
  explicit: AiModelRef | null | undefined,
  session: AiModelRef | null | undefined,
  savedDefault: AiModelRef | null | undefined,
  firstAvailable: AiModelRef | null | undefined,
): AiModelRef | null {
  return explicit ?? session ?? savedDefault ?? firstAvailable ?? null;
}
