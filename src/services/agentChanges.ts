import { invoke, isTauri } from "@tauri-apps/api/core";
import type { AgentFileChangeContent } from "../types/agentRun";

/** 按聊天和不可变工件 ID 请求后端校验过的历史前后快照。 */
export function readAgentFileChange(
  chatId: string,
  artifactId: string,
): Promise<AgentFileChangeContent> {
  if (!isTauri())
    return Promise.reject(
      new Error("历史文件快照仅在 GitMaster 桌面应用中可读取。"),
    );
  return invoke<AgentFileChangeContent>("agent_read_file_change", {
    id: chatId,
    artifactId,
  });
}
