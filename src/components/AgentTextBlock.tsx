import { previewLongText } from "../ui/agentPresentation";

/** 显示有界文本预览，并允许用户展开查看完整原文。 */
export function AgentTextBlock({
  text,
  className = "agent-message-text",
}: {
  text: string;
  className?: string;
}) {
  const view = previewLongText(text);
  return (
    <div className={className}>
      <div>{view.preview}</div>
      {view.truncated && (
        <details className="agent-long-text">
          <summary>展开完整内容</summary>
          <pre>{view.fullText}</pre>
        </details>
      )}
    </div>
  );
}
