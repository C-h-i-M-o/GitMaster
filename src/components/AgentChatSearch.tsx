import type { ChatSearchResult } from "../types/agentMemory";
import type { useAgentSearch } from "../hooks/useAgentSearch";

/** 展示按当前工作区及归档筛选的聊天标题与消息命中片段。 */
export function AgentChatSearch({
  search,
  onSelect,
}: {
  search: ReturnType<typeof useAgentSearch>;
  onSelect: (result: ChatSearchResult) => void;
}) {
  return (
    <section className="agent-chat-search" aria-label="搜索聊天记录">
      <label>
        搜索聊天
        <input
          type="search"
          aria-label="搜索聊天记录"
          maxLength={200}
          value={search.query}
          onChange={search.change}
          placeholder="搜索标题和消息"
        />
      </label>
      {search.busy && <small role="status">正在搜索…</small>}
      {search.error && <small role="alert">{search.error}</small>}
      {search.unavailable.length > 0 && (
        <small role="status">
          {search.unavailable.length} 个聊天无法搜索。
        </small>
      )}
      {search.query.trim() &&
        !search.busy &&
        !search.error &&
        search.results.length === 0 && (
          <small role="status">没有匹配的聊天。</small>
        )}
      {search.results.length > 0 && (
        <ul className="agent-chat-search-results">
          {search.results.map((result) => (
            <li key={`${result.chatId}:${result.sequence ?? "title"}`}>
              <button type="button" onClick={() => onSelect(result)}>
                <strong>{result.title}</strong>
                {result.snippet && <small>{result.snippet}</small>}
                {result.sequence !== null && (
                  <small>消息序号 {result.sequence}</small>
                )}
              </button>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
