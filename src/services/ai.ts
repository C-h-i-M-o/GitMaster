import { Channel, invoke, isTauri } from "@tauri-apps/api/core";
import type {
  AiCredentialAction,
  AiEventEnvelope,
  AiJobSnapshot,
  AiModelRef,
  AiPermission,
  AiProvider,
  AiRunStatus,
  AiSession,
  AiSettingsSnapshot,
  AiModelTestResult,
} from "../types/ai";

export interface AiSessionPage {
  session: AiSession;
  events: AiEventEnvelope[];
  status: AiRunStatus;
  nextSequence: number;
  more: boolean;
}
/** 浏览器预览不伪造模型或 Agent 执行结果。 */
function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  return isTauri()
    ? invoke<T>(command, args)
    : Promise.reject({ code: "AI_DESKTOP_REQUIRED" });
}
/** 将后端稳定码翻译为可操作提示，不显示原始服务商异常。 */
export function aiError(cause: unknown): string {
  const code =
    cause &&
    typeof cause === "object" &&
    "code" in cause &&
    typeof cause.code === "string"
      ? cause.code
      : "AI_RUNTIME_FAILED";
  const messages: Record<string, string> = {
    AI_SESSION_LIMIT: "会话数量达到上限，请先删除不需要的会话。",
    AI_CREDENTIAL_BUSY:
      "AI 任务运行期间不能更换、清除密钥或删除提供商。请停止任务后保存，当前输入仍保留。",
    AI_SETTINGS_TOO_LARGE: "模型配置过大，请减少提供商或模型数量。",
    AI_RUNTIME_STOPPED: "AI 进程已结束，请重新载入会话并核对任务状态。",
    AI_RUNTIME_NOT_STARTED: "AI 进程尚未启动，请重新载入会话。",
    AI_RUNTIME_STOPPING: "AI 进程正在回收，请稍后重新载入会话。",
    AI_DESKTOP_REQUIRED: "请在桌面应用中使用 AI。",
    AI_UNKNOWN_MODEL: "模型不可用，请重新选择或配置。",
    AI_INVALID_PROVIDER: "请检查提供商 ID、地址和模型列表。",
    AI_INVALID_MODEL: "请填写有效模型 ID 与容量，输出上限必须小于上下文。",
    AI_INVALID_URL: "API 地址无效，不能含凭据；远程明文 HTTP 需要明确确认。",
    AI_STALE_SETTINGS: "配置已被其他位置修改，输入已保留，请重新读取后重试。",
    AI_CREDENTIAL_IO: "系统凭据存储不可用，密钥未改存为明文。",
    AI_BUSY: "另一个 AI 任务仍在运行，请先停止或等待完成。",
    AI_UNSAVED_FILES: "存在未保存文件，AI 未执行写入。请保存后重新发送任务。",
    AI_DECISION_EXPIRED: "此请求已取消或过期，未执行操作。",
    AI_RUNTIME_MISSING: "AI 运行资源缺失，请重新构建或安装完整桌面包。",
    AI_RUNTIME_START: "AI 运行时无法启动，请检查安装资源。",
    AI_SESSION_PROJECT: "此会话属于其他项目，请切回所属项目。",
    AI_JOBS_RUNNING: "请先停止当前会话的后台任务，再切换权限。",
    AI_TIMEOUT: "等待 AI 运行时超时；请先核对任务状态，避免重复操作。",
    AI_CONFIRM_CLOSE: "会话中仍有任务，请确认结束后再删除。",
    SANDBOX_UNAVAILABLE: "本机沙箱不可用，受限命令没有执行。",
  };
  return messages[code] ?? `AI 操作未完成（${code}）。`;
}
/** 读取脱敏模型配置。 */
export function readAiSettings(): Promise<AiSettingsSnapshot> {
  return call("read_ai_settings");
}
/** 配置和密钥动作使用原生系统存储事务。 */
export function saveAiProvider(
  expectedRevision: number,
  provider: AiProvider,
  credentialAction: AiCredentialAction,
): Promise<AiSettingsSnapshot> {
  return call("save_ai_provider", {
    expectedRevision,
    provider,
    credentialAction,
  });
}
/** 删除提供商不删除历史会话。 */
export function deleteAiProvider(
  expectedRevision: number,
  providerId: string,
): Promise<AiSettingsSnapshot> {
  return call("delete_ai_provider", { expectedRevision, providerId });
}
/** 选择模型仅更新新会话默认值。 */
export function updateAiDefaults(
  expectedRevision: number,
  model: AiModelRef | null,
  permission: AiPermission,
): Promise<AiSettingsSnapshot> {
  return call("update_ai_defaults", { expectedRevision, model, permission });
}
/** 会话列表仅查询当前已打开项目。 */
export function listAiSessions(
  repositoryId: string,
  offset = 0,
): Promise<{ items: AiSession[]; nextOffset: number | null }> {
  return call("list_ai_sessions", { repositoryId, offset });
}
/** 创建会话时工作目录由 Rust 解析。 */
export function createAiSession(
  repositoryId: string,
  model: AiModelRef,
  permission: AiPermission,
): Promise<AiSession> {
  return call("create_ai_session", { repositoryId, model, permission });
}
/** 按事件游标分页读取恢复历史。 */
export function readAiSession(
  repositoryId: string,
  sessionId: string,
  after: number,
): Promise<AiSessionPage> {
  return call("read_ai_session", { repositoryId, sessionId, after });
}
/** 通道仅投递此会话事件，释放回调后不再更新组件。 */
export async function subscribeAiEvents(
  sessionId: string,
  onEvent: (event: AiEventEnvelope) => void,
): Promise<() => void> {
  const channel = new Channel<AiEventEnvelope>();
  channel.onmessage = onEvent;
  await call("subscribe_ai_events", { sessionId, channel });
  return () => {
    channel.onmessage = () => {};
  };
}
/** 客户端消息 ID 用于丢失响应后的幂等接纳。 */
export function sendAiMessage(
  repositoryId: string,
  sessionId: string,
  clientMessageId: string,
  text: string,
  hasUnsavedFiles: boolean,
): Promise<{ runId: string }> {
  return call("send_ai_message", {
    repositoryId,
    sessionId,
    clientMessageId,
    text,
    hasUnsavedFiles,
  });
}
/** 取消只请求停止，终态仍等待后端通知。 */
export function cancelAiRun(sessionId: string, runId: string): Promise<void> {
  return call("cancel_ai_run", { sessionId, runId });
}
/** 会话空闲时才更新模型、权限或标题。 */
export function updateAiSession(
  sessionId: string,
  patch: { title?: string; model?: AiModelRef; permission?: AiPermission },
): Promise<AiSession> {
  return call("update_ai_session", { sessionId, patch });
}
/** 删除运行中会话必须明确确认结束任务。 */
export function deleteAiSession(
  sessionId: string,
  confirmed: boolean,
): Promise<void> {
  return call("delete_ai_session", { sessionId, confirmed });
}
/** 单次审批不写入任何永久的权限例外。 */
export function resolveAiApproval(
  sessionId: string,
  approvalId: string,
  decision: "allowed-once" | "rejected",
): Promise<void> {
  return call("resolve_ai_approval", { sessionId, approvalId, decision });
}
/** 用户问题的文本回答和审批动作使用不同接口。 */
export function answerAiQuestion(
  sessionId: string,
  questionId: string,
  answer: { answers: { id: string; selected: string[]; custom?: string }[] },
): Promise<void> {
  return call("answer_ai_question", { sessionId, questionId, answer });
}
/** 独立 UI 输出读取不会耗掉模型的 job_output 游标。 */
export function readAiJobs(sessionId: string): Promise<AiJobSnapshot[]> {
  return call("read_ai_jobs", { sessionId });
}
/** 全局任务状态不泄露其他会话的内容。 */
export function readAiActivity(): Promise<{ busy: boolean }> {
  return call("read_ai_activity");
}
/** 仅在用户点击时验证已保存模型，可能产生 API 费用。 */
export function testAiProvider(
  providerId: string,
  modelId: string,
): Promise<AiModelTestResult> {
  return call("test_ai_provider", { providerId, modelId });
}
/** 任务终态仍由后续快照确认。 */
export function stopAiJob(sessionId: string, jobId: string): Promise<void> {
  return call("stop_ai_job", { sessionId, jobId });
}
/** 使用上游基础压缩器，不删除原始日志。 */
export function compactAiSession(
  sessionId: string,
): Promise<{ runId: string }> {
  return call("compact_ai_session", { sessionId });
}
