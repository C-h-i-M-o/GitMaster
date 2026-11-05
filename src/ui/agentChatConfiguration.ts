import type {
  AgentWorkspace,
  ChatSession,
  ModelConfig,
  PermissionMode,
} from "../types/agent";
import type { ModelParameters } from "../types/agentModels";

/** 当前会话完整配置草稿；参数为 null 时继承所选模型默认值。 */
export interface ChatConfigurationDraft {
  modelConfigId: string | null;
  permissionMode: PermissionMode;
  parameters: ModelParameters | null;
}

/** 从持久会话复制配置，避免编辑时直接修改读取快照。 */
export function chatConfigurationForSession(
  session: ChatSession,
): ChatConfigurationDraft {
  return {
    modelConfigId: session.modelConfigId || null,
    permissionMode: session.permissionMode,
    parameters: session.parameters ? { ...session.parameters } : null,
  };
}

/** 判断会话配置草稿是否已与当前持久快照一致。 */
export function chatConfigurationMatchesSession(
  draft: ChatConfigurationDraft,
  session: ChatSession,
): boolean {
  if (
    draft.modelConfigId !== (session.modelConfigId || null) ||
    draft.permissionMode !== session.permissionMode
  )
    return false;
  if (draft.parameters === null || session.parameters === null)
    return draft.parameters === session.parameters;
  const keys: (keyof ModelParameters)[] = [
    "contextLimit",
    "maxOutputTokens",
    "thinkingMode",
    "reasoningEffort",
    "thinkingBudgetTokens",
    "temperature",
    "topP",
  ];
  return keys.every(
    (key) => draft.parameters?.[key] === session.parameters?.[key],
  );
}

/** 为新会话准备草稿，模型参数整体继承模型默认值。 */
export function newChatConfiguration(
  modelConfigId: string | null,
  permissionMode: PermissionMode,
): ChatConfigurationDraft {
  return { modelConfigId, permissionMode, parameters: null };
}

/** 工作区切换模型时明确丢弃旧模型参数快照。 */
export function chatConfigurationForModel(
  draft: ChatConfigurationDraft,
  modelConfigId: string | null,
): ChatConfigurationDraft {
  return { ...draft, modelConfigId, parameters: null };
}

/** 根据草稿参数或模型默认值提供参数编辑器的完整值。 */
export function effectiveChatParameters(
  draft: ChatConfigurationDraft,
  model: ModelConfig,
): ModelParameters {
  return { ...(draft.parameters ?? model.defaults) };
}

/** 比较完整参数快照，等同模型默认值时恢复整体继承状态。 */
export function normalizeChatParameters(
  parameters: ModelParameters,
  model: ModelConfig,
): ModelParameters | null {
  const keys: (keyof ModelParameters)[] = [
    "contextLimit",
    "maxOutputTokens",
    "thinkingMode",
    "reasoningEffort",
    "thinkingBudgetTokens",
    "temperature",
    "topP",
  ];
  return keys.every((key) => parameters[key] === model.defaults[key])
    ? null
    : { ...parameters };
}

/** 新聊天参数编辑以模型默认值为基线，并保留完整会话快照语义。 */
export function updateChatParameter<K extends keyof ModelParameters>(
  draft: ChatConfigurationDraft,
  model: ModelConfig,
  field: K,
  value: ModelParameters[K],
): ChatConfigurationDraft {
  const parameters = effectiveChatParameters(draft, model);
  parameters[field] = value;
  return {
    ...draft,
    parameters: normalizeChatParameters(parameters, model),
  };
}

/** 恢复单个参数为模型默认值，完全一致时重新继承整组默认值。 */
export function restoreChatParameter<K extends keyof ModelParameters>(
  draft: ChatConfigurationDraft,
  model: ModelConfig,
  field: K,
): ChatConfigurationDraft {
  return updateChatParameter(draft, model, field, model.defaults[field]);
}

/** 仅返回可执行的默认工作区和唯一当前文件夹工作区。 */
export function currentWorkspaces(
  workspaces: AgentWorkspace[],
): AgentWorkspace[] {
  const defaultWorkspace = workspaces.find(
    (workspace) => workspace.id === "default" && workspace.kind === "default",
  );
  const folder = workspaces.find((workspace) => workspace.kind === "folder");
  return [defaultWorkspace, folder].filter(
    (workspace): workspace is AgentWorkspace => workspace !== undefined,
  );
}

/** 路径不一致的旧文件夹会话保留可读，但不允许继续发送。 */
export function sessionWorkspace(
  session: ChatSession,
  workspaces: AgentWorkspace[],
): AgentWorkspace | null {
  const workspace = workspaces.find((item) => item.id === session.workspaceId);
  if (!workspace) return null;
  if (workspace.kind === "default") return workspace;
  const path = (value: string | null): string | null => {
    if (!value) return null;
    return value
      .replace(/[\\/]+$/, "")
      .replaceAll("/", "\\")
      .toLowerCase();
  };
  return path(workspace.rootPath) === path(session.repositoryPath)
    ? workspace
    : null;
}
