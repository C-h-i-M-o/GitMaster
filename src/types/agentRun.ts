/** 历史修改只携带不可变工件引用，实际文件内容按需读取。 */
export interface AgentFileChange {
  artifactId: string;
  digest: string;
  path: string;
  beforeVersion: string | null;
  afterVersion: string;
  created: boolean;
}
export interface AgentFileChangeContent {
  path: string;
  beforeContent: string | null;
  afterContent: string;
  beforeVersion: string | null;
  afterVersion: string;
  verified: boolean;
}

/** 供应商未返回的计数保持 null；缓存和推理是输入/输出的子集。 */
export interface ModelUsage {
  inputTokens: number | null;
  outputTokens: number | null;
  cachedInputTokens: number | null;
  cacheCreationInputTokens: number | null;
  reasoningTokens: number | null;
}
export interface ModelAttempt {
  id: string;
  attempt: number;
  phase: "started" | "completed" | "failed";
  usage: ModelUsage | null;
  errorCode: string | null;
}
/** UTF-8 请求字节的保守估算，并非供应商精确 token 计数。 */
export interface ContextPreview {
  estimatedInputTokens: number;
  inputBudget: number;
  contextLimit: number;
  outputReserve: number;
  safetyReserve: number;
  checkpointThrough: number | null;
  truncatedToolSequences: number[];
}
