import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  AgentSettingsView,
  ChatList,
  ChatSession,
  ChatView,
  DeviceUsage,
  PermissionMode,
  RunNotice,
  SaveProfile,
} from "../types/agent";
/** 浏览器不伪造桌面能力，也不将 Key 保存到 localStorage。 */
function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauri())
    return Promise.reject({
      code: "AGENT_DESKTOP_REQUIRED",
      message: "请在 GitMaster 桌面应用中使用 AI。",
    });
  return invoke<T>(command, args);
}
export const agentApi = {
  settings: (): Promise<AgentSettingsView> => call("agent_read_settings"),
  save: (request: SaveProfile): Promise<AgentSettingsView> =>
    call("agent_save_profile", { request }),
  deleteProfile: (id: string): Promise<AgentSettingsView> =>
    call("agent_delete_profile", { id }),
  select: (id: string): Promise<AgentSettingsView> =>
    call("agent_select_profile", { id }),
  permission: (mode: PermissionMode): Promise<AgentSettingsView> =>
    call("agent_set_permission", { mode }),
  test: (id: string): Promise<void> => call("agent_test_connection", { id }),
  chats: (): Promise<ChatList> => call("agent_list_chats"),
  create: (): Promise<ChatSession> => call("agent_create_chat"),
  read: (id: string): Promise<ChatView> => call("agent_read_chat", { id }),
  update: (
    id: string,
    title: string,
    archived: boolean,
  ): Promise<ChatSession> => call("agent_update_chat", { id, title, archived }),
  delete: (id: string): Promise<void> => call("agent_delete_chat", { id }),
  send: (id: string, text: string): Promise<string> =>
    call("agent_send_message", { id, text }),
  cancel: (id: string, runId: string): Promise<void> =>
    call("agent_cancel_run", { id, runId }),
  /** 关闭前等待所有聊天停止及终态落盘。 */
  shutdown: (): Promise<void> => call("agent_shutdown"),
  resolve: (
    id: string,
    runId: string,
    approvalId: string,
    approved: boolean,
  ): Promise<void> =>
    call("agent_resolve_approval", { id, runId, approvalId, approved }),
  usage: (): Promise<DeviceUsage> => call("agent_read_device_usage"),
};
/** 仅订阅安全事件；订阅失败由调用方显示。 */
export function listenAgent(onEvent: (notice: RunNotice) => void) {
  return listen<RunNotice>("agent:event", (event) => onEvent(event.payload));
}
/** 写操作结果触发一次工作台真实状态刷新。 */
export function listenAgentWrites(refresh: () => void) {
  return listen("agent:repository-changed", refresh);
}
/** 只显示后端结构化提示，不把未知异常或输入回显到界面/日志。 */
export function agentError(error: unknown): string {
  if (
    typeof error === "object" &&
    error !== null &&
    "message" in error &&
    typeof error.message === "string"
  )
    return error.message;
  return "AI 操作未完成，请重试。";
}
