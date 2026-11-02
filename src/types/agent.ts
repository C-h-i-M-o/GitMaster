/** Agent IPC 只使用可序列化 JSON，已保存密钥不出现在任何读取类型中。 */
export type JsonValue =
  null | boolean | number | string | JsonValue[] | { [key: string]: JsonValue };
export type ProviderProtocol =
  "chatCompletions" | "responses" | "anthropicMessages";
export type PermissionMode = "requestApproval" | "autoApprove" | "fullAccess";
export interface ProviderProfile {
  id: string;
  name: string;
  protocol: ProviderProtocol;
  baseUrl: string;
  model: string;
  credentialId: string;
}
export interface AgentSettingsView {
  settings: {
    version: number;
    profiles: ProviderProfile[];
    activeProfileId: string | null;
    defaultPermissionMode: PermissionMode;
  };
  hasApiKeys: Record<string, boolean>;
  warnings: string[];
}
export interface SaveProfile {
  id: string | null;
  name: string;
  protocol: ProviderProtocol;
  baseUrl: string;
  model: string;
  apiKey: string | null;
}
export interface ChatSession {
  version: number;
  id: string;
  title: string;
  repositoryPath: string | null;
  profileId: string;
  permissionMode: PermissionMode;
  archived: boolean;
  createdAt: number;
  updatedAt: number;
}
export interface ToolCall {
  id: string;
  name: string;
  arguments: JsonValue;
}
export interface ApprovalRequest {
  id: string;
  call: ToolCall;
  reason: string;
  target: string;
  preview: JsonValue;
}
export type EventPayload =
  | {
      kind: "runStarted";
      profile: ProviderProfile;
      permissionMode: PermissionMode;
    }
  | { kind: "userMessage"; text: string }
  | {
      kind: "assistantMessage";
      text: string;
      protocol: ProviderProtocol;
      wire: JsonValue;
      calls: ToolCall[];
    }
  | { kind: "toolCall"; call: ToolCall }
  | { kind: "approvalRequested"; approval: ApprovalRequest }
  | { kind: "approvalDecided"; approvalId: string; approved: boolean }
  | {
      kind: "toolResult";
      callId: string;
      name: string;
      result: JsonValue;
      isError: boolean;
    }
  | {
      kind: "summaryEvidence";
      callId: string;
      evidence: JsonValue;
      relatedChats: string[];
    }
  | {
      kind: "runEnded";
      status: "completed" | "cancelled" | "failed" | "interrupted";
      message: string | null;
    };
export type AgentEvent = {
  version: number;
  sequence: number;
  sessionId: string;
  runId: string;
  timestamp: number;
} & EventPayload;
export interface ChatRecord {
  session: ChatSession;
  events: AgentEvent[];
}
export interface ChatList {
  chats: ChatSession[];
  unavailable: string[];
}
export interface RunSnapshot {
  runId: string;
  phase: string;
  approval: ApprovalRequest | null;
  text: string;
}
export interface ChatView {
  record: ChatRecord;
  run: RunSnapshot | null;
}
export interface AgentError {
  code: string;
  message: string;
}
export type RunNotice =
  | { kind: "record"; event: AgentEvent }
  | { kind: "progress"; sessionId: string; state: RunSnapshot }
  | {
      kind: "finished";
      sessionId: string;
      runId: string;
      error: AgentError | null;
    };
export interface DeviceUsage {
  totalBytes: number;
  categories: { id: string; label: string; bytes: number }[];
  locations: string[];
}
