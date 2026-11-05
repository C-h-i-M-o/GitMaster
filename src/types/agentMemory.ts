/** 用户明确保存的本地记忆；原始凭据不进入该结构。 */
export interface AgentMemory {
  id: string;
  scope: "workspace" | "user";
  workspaceId: string | null;
  text: string;
  tags: string[];
  source: { chatId: string | null; sequence: number | null };
  enabled: boolean;
  revision: number;
  createdAt: number;
  updatedAt: number;
}
export interface SaveMemory extends Omit<
  AgentMemory,
  "id" | "revision" | "createdAt" | "updatedAt"
> {
  id: string | null;
  expectedRevision: number | null;
}
export interface ChatMemoryConfig {
  enabled: boolean;
  includeUser: boolean;
  excludedIds: string[];
}
export interface MemorySelection {
  selected: AgentMemory[];
  excluded: {
    id: string;
    reason: "disabled" | "userExcluded" | "budget" | "noMatch";
  }[];
  estimatedTokens: number;
}
export interface ChatSearchResult {
  chatId: string;
  title: string;
  sequence: number | null;
  snippet: string | null;
  updatedAt: number;
}
export interface ChatSearchResults {
  results: ChatSearchResult[];
  unavailable: string[];
}
