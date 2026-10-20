import { invoke, isTauri } from "@tauri-apps/api/core";
import type {
  WorkspaceSession,
  WorkspaceSessionsFile,
} from "../types/workspaceSession";

/** 读取桌面端保存的工作区会话；浏览器预览返回空结构。 */
export async function readWorkspaceSessions(): Promise<WorkspaceSessionsFile> {
  if (!isTauri()) return { version: 1, lastRootPath: null, projects: [] };
  return invoke<WorkspaceSessionsFile>("read_workspace_sessions");
}

/** 保存当前项目的工作区会话。 */
export async function writeWorkspaceSession(
  session: WorkspaceSession,
): Promise<void> {
  if (!isTauri()) return;
  await invoke("write_workspace_session", { session });
}
