import type { AgentTranscriptReferences } from "../ui/agentTranscript";
import type { AgentTranscriptMemorySuggestion } from "../ui/agentTranscript";
import { agentReferenceSource } from "../ui/agentChanges";
import { AgentMemorySuggestion } from "./AgentMemorySuggestion";
import "./AgentRunContext.css";

/** 折叠展示当前运行的记忆建议和用户提供的历史引用。 */
export function AgentRunContext({
  chatId,
  workspaceId,
  suggestions,
  referenceGroups,
  onMemorySaved,
}: {
  chatId: string;
  workspaceId: string;
  suggestions: AgentTranscriptMemorySuggestion[];
  referenceGroups: AgentTranscriptReferences[];
  onMemorySaved?: () => void;
}) {
  if (suggestions.length === 0 && referenceGroups.length === 0) return null;
  return (
    <section className="agent-run-context" aria-label="本轮上下文资料">
      {suggestions.length > 0 && (
        <details>
          <summary>记忆建议 · {suggestions.length}</summary>
          {suggestions.map((suggestion) => (
            <AgentMemorySuggestion
              key={suggestion.sequence}
              chatId={chatId}
              workspaceId={workspaceId}
              sequence={suggestion.sequence}
              text={suggestion.text}
              tags={suggestion.tags}
              onSaved={onMemorySaved}
            />
          ))}
        </details>
      )}
      {referenceGroups.map((group) => (
        <details key={group.sequence}>
          <summary>用户提供的历史引用 · 事件 {group.sequence}</summary>
          <p>以下内容是运行时引用的历史资料，不代表文件当前内容。</p>
          {group.references.map((reference, index) => (
            <article key={`${group.sequence}-${index}`}>
              <strong>{reference.label}</strong>
              <p>{agentReferenceSource(reference)}</p>
              <pre>{reference.text}</pre>
            </article>
          ))}
        </details>
      ))}
    </section>
  );
}
