import { useAgentMemorySuggestion } from "../hooks/useAgentMemorySuggestion";
import "./agentMemory.css";

/** 展示一条需用户明确确认的记忆建议，不自动写入本地记忆。 */
export function AgentMemorySuggestion({
  chatId,
  workspaceId,
  sequence,
  text,
  tags,
  onSaved,
}: {
  chatId: string;
  workspaceId: string;
  sequence: number;
  text: string;
  tags: string[];
  onSaved?: () => void;
}) {
  const memory = useAgentMemorySuggestion({
    chatId,
    workspaceId,
    sequence,
    text,
    tags,
    onSaved,
  });
  return (
    <aside className="agent-memory-suggestion" aria-label="记忆建议">
      <strong>记忆建议</strong>
      <p>{text}</p>
      {tags.length > 0 && <small>标签：{tags.join("、")}</small>}
      <label>
        <input
          type="checkbox"
          checked={memory.scope === "user"}
          onChange={memory.scopeChanged}
          disabled={memory.saving || memory.saved}
        />
        保存为跨工作区用户偏好
      </label>
      <small>仅在此显式选择后，其他工作区中也允许的聊天才可检索此条目。</small>
      {memory.error && <p role="alert">{memory.error}</p>}
      <button
        type="button"
        className="secondary"
        onClick={memory.saveClicked}
        disabled={memory.saving || memory.saved}
      >
        {memory.saved ? "已保存" : memory.saving ? "保存中…" : "保存此记忆"}
      </button>
    </aside>
  );
}
