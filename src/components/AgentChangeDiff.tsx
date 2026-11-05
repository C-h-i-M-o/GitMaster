import { useAgentChangeDiff } from "../hooks/useAgentChangeDiff";
import "./AgentChanges.css";

/** 只展示已通过后端验证的单次写入快照差异。 */
export default function AgentChangeDiff({
  artifactId,
  beforeContent,
  afterContent,
}: {
  artifactId: string;
  beforeContent: string;
  afterContent: string;
}) {
  const container = useAgentChangeDiff(artifactId, beforeContent, afterContent);
  return (
    <div
      className="agent-change-diff"
      ref={container}
      aria-label="历史前后差异"
    />
  );
}
