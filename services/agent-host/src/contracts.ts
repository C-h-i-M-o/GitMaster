/** 首版固定协议，与上游 llm-pi-ai 的协议标识保持一致。 */
export type AiProtocol =
  "openai-completions" | "openai-responses" | "anthropic-messages";
/** 权限分别约束会话，不能从项目文件隐式授予。 */
export type AiPermission =
  "read-only" | "workspace-write" | "danger-full-access";
/** 模型声明的容量与可选推理档位。 */
export interface AiModel {
  modelId: string;
  displayName: string;
  contextWindow: number;
  maxOutputTokens: number;
  reasoningEfforts?: Record<string, string | null>;
}
/** 普通配置仅包含凭据引用。 */
export interface AiProvider {
  providerId: string;
  displayName: string;
  protocol: AiProtocol;
  baseUrl: string;
  credentialRef: string | null;
  allowInsecureHttp: boolean;
  models: AiModel[];
}
/** 会话保存的确切模型选择。 */
export interface AiModelRef {
  providerId: string;
  modelId: string;
  reasoningEffort: string | null;
}
/** AI 配置独立于现有 Git 设置迁移。 */
export interface AiSettings {
  version: 1;
  revision: number;
  providers: AiProvider[];
  defaultModel: AiModelRef | null;
  defaultPermission: AiPermission;
}
/** 前端只接收密钥是否可用，绝不读取已有明文。 */
export interface AiSettingsSnapshot {
  settings: AiSettings;
  credentials: Record<string, "configured" | "missing" | "unavailable">;
  warning?: string;
}
/** 密钥留空不等于删除。 */
export type AiCredentialAction =
  { kind: "keep" } | { kind: "replace"; value: string } | { kind: "remove" };

/** 公开错误只包含识别后的事实，不携带提供商响应或凭据。 */
export interface AiFailure {
  code: string;
  message: string;
}
/** 连接失败属于测试结果，具体原因不依赖通用 RPC 异常通道。 */
export type AiModelTestResult = { ok: true } | { ok: false; error: AiFailure };
/** 与进程生命周期分离的持久会话目录。 */
export interface AiSession {
  sessionId: string;
  workspaceRoot: string;
  repositoryKey: string;
  title: string;
  model: AiModelRef;
  permission: AiPermission;
  createdAt: string;
  updatedAt: string;
}
/** Agent 状态由后端事实驱动。 */
export type AiRunStatus =
  | "idle"
  | "running"
  | "awaitingApproval"
  | "awaitingAnswer"
  | "stopping"
  | "failed";
/** 渲染用事件不承载凭据和完整运行时配置。 */
export interface AiEventEnvelope {
  turn?: number | null;
  step?: number | null;
  time?: number;
  runtimeEpoch: string;
  sessionId: string;
  sequence: number;
  runId: string | null;
  event: AiEvent;
}
/** 工具输出和助手消息使用判别联合，避免前端猜测数据形状。 */
export type AiEvent =
  | {
      kind: "turn";
      turn: number;
      state: "running" | "completed" | "failed" | "cancelled" | "interrupted";
    }
  | { kind: "status"; status: AiRunStatus; reason?: string }
  | { kind: "user"; text: string }
  | {
      kind: "assistant";
      messageId: string;
      text: string;
      final: boolean;
      reasoning?: string;
    }
  | {
      kind: "tool";
      toolCallId: string;
      name: string;
      input: string;
      output: string | null;
      failed: boolean;
      state?: "cancelled" | "interrupted";
    }
  | {
      kind: "approval";
      approvalId: string;
      title: string;
      detail: string;
      withdrawn: boolean;
    }
  | {
      kind: "question";
      questionId: string;
      title: string;
      detail: string;
      withdrawn: boolean;
    }
  | { kind: "job"; jobId: string; label: string; state: string; output: string }
  | { kind: "error"; code: string; message: string };

/** 命令行任务列表只包含当前会话的后端投影。 */
export interface AiJobSnapshot {
  jobId: string;
  label: string;
  state: string;
  output: string;
  command: string;
  workingDirectory: string;
  detail: string | null;
  toolCallId: string | null;
}
