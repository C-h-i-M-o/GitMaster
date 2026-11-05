import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  AgentSettingsView,
  AgentWorkspace,
  ChatList,
  ChatSession,
  ChatView,
  DeviceUsage,
  PermissionMode,
  RunNotice,
  SaveProfile,
} from "../types/agent";
import type {
  ConnectionTestReport,
  DiscoveredModelPage,
  ModelParameters,
  ModelDraft,
  ProviderDraft,
  SaveModel,
  SaveProvider,
} from "../types/agentModels";
import type {
  AgentMemory,
  ChatMemoryConfig,
  ChatSearchResults,
  MemorySelection,
  SaveMemory,
} from "../types/agentMemory";
import type {
  ContextPreviewResult,
  UserReferenceSpec,
  VerifiedReference,
} from "../types/agentContext";
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
  saveProvider: (request: SaveProvider): Promise<AgentSettingsView> =>
    call("agent_save_provider", { request }),
  saveModel: (request: SaveModel): Promise<AgentSettingsView> =>
    call("agent_save_model", { request }),
  deleteProvider: (
    id: string,
    expectedRevision: number,
  ): Promise<AgentSettingsView> =>
    call("agent_delete_provider", { id, expectedRevision }),
  listModels: (
    provider: ProviderDraft,
    requestId: string,
    cursor: string | null,
  ): Promise<DiscoveredModelPage> =>
    call("agent_list_models", { provider, requestId, cursor }),
  testModel: (
    provider: ProviderDraft,
    model: ModelDraft,
    requestId: string,
  ): Promise<ConnectionTestReport> =>
    call("agent_test_model", { provider, model, requestId }),
  cancelConfiguration: (requestId: string): Promise<void> =>
    call("agent_cancel_configuration_request", { requestId }),
  deleteProfile: (id: string): Promise<AgentSettingsView> =>
    call("agent_delete_profile", { id }),
  select: (id: string): Promise<AgentSettingsView> =>
    call("agent_select_profile", { id }),
  permission: (mode: PermissionMode): Promise<AgentSettingsView> =>
    call("agent_set_permission", { mode }),
  test: (id: string): Promise<void> => call("agent_test_connection", { id }),
  chats: (): Promise<ChatList> => call("agent_list_chats"),
  create: (): Promise<ChatSession> =>
    call("agent_create_chat", { workspaceId: null, modelConfigId: null }),
  createChat: (
    workspaceId: string | null,
    modelConfigId: string | null,
    permissionMode?: PermissionMode,
    parameters?: ModelParameters | null,
  ): Promise<ChatSession> =>
    call("agent_create_chat", {
      workspaceId,
      modelConfigId,
      ...(permissionMode ? { permissionMode } : {}),
      ...(parameters === undefined ? {} : { parameters }),
    }),
  configureChat: (request: {
    id: string;
    modelConfigId: string | null;
    permissionMode: PermissionMode;
    parameters: ModelParameters | null;
  }): Promise<ChatSession> => call("agent_configure_chat", request),
  listWorkspaces: (): Promise<AgentWorkspace[]> =>
    call("agent_list_workspaces"),
  chooseFolder: (): Promise<AgentWorkspace[]> => call("agent_choose_folder"),
  listMemories: (
    workspaceId: string,
    includeUser: boolean,
  ): Promise<AgentMemory[]> =>
    call("agent_list_memories", { workspaceId, includeUser }),
  saveMemory: (request: SaveMemory): Promise<AgentMemory> =>
    call("agent_save_memory", { request }),
  deleteMemory: (id: string, expectedRevision: number): Promise<void> =>
    call("agent_delete_memory", { id, expectedRevision }),
  selectMemories: (
    workspaceId: string,
    includeUser: boolean,
    query: string,
    excludedIds: string[],
    budgetTokens = 2048,
  ): Promise<MemorySelection> =>
    call("agent_select_memories", {
      workspaceId,
      includeUser,
      query,
      excludedIds,
      budgetTokens,
    }),
  setChatMemory: (
    id: string,
    request: ChatMemoryConfig,
  ): Promise<ChatSession> => call("agent_set_chat_memory", { id, request }),
  searchChats: (
    workspaceId: string,
    query: string,
    archived: boolean,
  ): Promise<ChatSearchResults> =>
    call("agent_search_chats", { workspaceId, query, archived }),
  previewReferences: (
    workspaceId: string,
    references: UserReferenceSpec[],
  ): Promise<VerifiedReference[]> =>
    call("agent_preview_references", { workspaceId, references }),
  previewContext: (
    id: string,
    text: string,
    references?: UserReferenceSpec[],
  ): Promise<ContextPreviewResult> =>
    call("agent_preview_context", {
      id,
      text,
      ...(references ? { references } : {}),
    }),
  read: (id: string): Promise<ChatView> => call("agent_read_chat", { id }),
  update: (
    id: string,
    title: string,
    archived: boolean,
  ): Promise<ChatSession> => call("agent_update_chat", { id, title, archived }),
  delete: (id: string): Promise<void> => call("agent_delete_chat", { id }),
  send: (
    id: string,
    text: string,
    references?: UserReferenceSpec[],
  ): Promise<string> =>
    call("agent_send_message", {
      id,
      text,
      ...(references ? { references } : {}),
    }),
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
/** 工作区变化后由调用方重读后端签发的工作区列表。 */
export function listenAgentWorkspacesChanged(refresh: () => void) {
  return listen("agent:workspaces-changed", refresh);
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
