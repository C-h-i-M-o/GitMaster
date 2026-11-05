import type {
  UserReferenceSpec,
  VerifiedReference,
} from "../types/agentContext";

export const MAX_AGENT_REFERENCES = 8;
export const MAX_AGENT_REFERENCE_BYTES = 64 * 1024;

/** 在前端阻止超过数量或 UTF-8 字节预算的引用集合。 */
export function referenceBatchError(
  references: readonly UserReferenceSpec[],
): string | null {
  if (references.length > MAX_AGENT_REFERENCES)
    return "每条消息最多选择 8 项引用。";
  const byteLength = new TextEncoder().encode(
    JSON.stringify(references),
  ).byteLength;
  return byteLength > MAX_AGENT_REFERENCE_BYTES
    ? "引用内容总量超过 64 KiB，请缩短文本或减少引用。"
    : null;
}

/** 将已校验版本绑定到原引用请求，供发送时再次验证。 */
export function bindReferenceVersions(
  requested: readonly UserReferenceSpec[],
  verified: readonly VerifiedReference[],
): UserReferenceSpec[] | null {
  if (requested.length !== verified.length) return null;
  const bound: UserReferenceSpec[] = [];
  for (const [index, reference] of requested.entries()) {
    const result = verified[index];
    if (
      !result ||
      !result.version ||
      result.kind !== reference.kind ||
      (reference.kind === "file" &&
        (result.path !== reference.path ||
          (result.startLine ?? null) !== (reference.startLine ?? null) ||
          (result.endLine ?? null) !== (reference.endLine ?? null)))
    )
      return null;
    bound.push({ ...reference, expectedVersion: result.version });
  }
  return bound;
}

/** 生成隔离工作区、聊天、输入和引用内容的请求代次键。 */
export function agentContextRequestKey(
  workspaceId: string,
  chatId: string | null,
  text: string,
  references: readonly UserReferenceSpec[],
  configurationKey: string,
  memoryRevision: number,
): string {
  return JSON.stringify([
    workspaceId,
    chatId,
    text,
    references,
    configurationKey,
    memoryRevision,
  ]);
}

/** 判断切换聊天或工作区时是否为预览创建的新聊天。 */
export function contextScopeTransition(
  previous: { workspaceId: string; chatId: string | null },
  next: { workspaceId: string; chatId: string | null },
  preserveForCreate: boolean,
): {
  preserveReferences: boolean;
  clearReferenceBusy: true;
  clearContextBusy: true;
} {
  const preserveReferences =
    preserveForCreate &&
    previous.workspaceId === next.workspaceId &&
    previous.chatId === null &&
    next.chatId !== null;
  return {
    preserveReferences,
    clearReferenceBusy: true,
    clearContextBusy: true,
  };
}
