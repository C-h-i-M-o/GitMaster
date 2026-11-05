import type {
  AgentMemory,
  ChatMemoryConfig,
  SaveMemory,
} from "../types/agentMemory";
import type { ChatSession } from "../types/agent";

/** 默认仅使用当前工作区记忆，不跨工作区读取用户偏好。 */
export function defaultChatMemoryConfig(): ChatMemoryConfig {
  return { enabled: true, includeUser: false, excludedIds: [] };
}

/** 检查聊天记忆配置草稿是否等同已保存快照。 */
export function chatMemoryMatchesSession(
  config: ChatMemoryConfig,
  session: ChatSession,
): boolean {
  if (
    config.enabled !== session.memory.enabled ||
    config.includeUser !== session.memory.includeUser ||
    config.excludedIds.length !== session.memory.excludedIds.length
  )
    return false;
  const excluded = new Set(session.memory.excludedIds);
  return config.excludedIds.every((id) => excluded.has(id));
}

/** 将已保存条目转换为带 revision 的编辑请求。 */
export function memoryEditRequest(entry: AgentMemory): SaveMemory {
  return {
    id: entry.id,
    expectedRevision: entry.revision,
    scope: entry.scope,
    workspaceId: entry.workspaceId,
    text: entry.text,
    tags: [...entry.tags],
    source: { ...entry.source },
    enabled: entry.enabled,
  };
}

/** 将一次运行中的记忆建议转为来源绑定的手工保存请求。 */
export function memorySuggestionRequest(
  chatId: string,
  workspaceId: string,
  sequence: number,
  text: string,
  tags: string[],
  scope: "workspace" | "user",
): SaveMemory {
  return {
    id: null,
    expectedRevision: null,
    scope,
    workspaceId: scope === "user" ? null : workspaceId,
    text: text.trim(),
    tags: [...new Set(tags.map((tag) => tag.trim()).filter(Boolean))],
    source: { chatId, sequence },
    enabled: true,
  };
}

/** 解析标签输入并按大小写不敏感去重。 */
export function parseMemoryTags(value: string): string[] {
  const seen = new Set<string>();
  return value
    .split(",")
    .map((tag) => tag.trim())
    .filter((tag) => {
      const key = tag.toLocaleLowerCase();
      if (!tag || seen.has(key)) return false;
      seen.add(key);
      return true;
    });
}

/** 将后端记忆筛选原因转换为可读中文。 */
export function memoryExclusionReason(
  reason: "disabled" | "userExcluded" | "budget" | "noMatch",
): string {
  switch (reason) {
    case "disabled":
      return "已停用";
    case "userExcluded":
      return "本聊天已排除";
    case "budget":
      return "超出预览预算";
    case "noMatch":
      return "与检索内容无匹配";
  }
}
