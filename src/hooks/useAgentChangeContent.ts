import { useRef, useState } from "react";
import type {
  AgentFileChange,
  AgentFileChangeContent,
} from "../types/agentRun";
import { readAgentFileChange } from "../services/agentChanges";
import { agentChangeContentMatches } from "../ui/agentChanges";

const SNAPSHOT_ERROR = "无法验证本次写入的历史快照，未显示差异。";

/** 按用户展开动作读取单个快照，并拒绝未验证或与事件不匹配的内容。 */
export function useAgentChangeContent(chatId: string, change: AgentFileChange) {
  const [expanded, setExpanded] = useState(false);
  const [loading, setLoading] = useState(false);
  const [content, setContent] = useState<AgentFileChangeContent | null>(null);
  const [error, setError] = useState<string | null>(null);
  const loadingRef = useRef(false);

  /** 只在展开时通过独立 IPC 读取该工件，并在本地复核事件绑定。 */
  async function load(): Promise<void> {
    if (loadingRef.current) return;
    loadingRef.current = true;
    setLoading(true);
    setError(null);
    try {
      const snapshot = await readAgentFileChange(chatId, change.artifactId);
      if (!agentChangeContentMatches(change, snapshot)) {
        setContent(null);
        setError(SNAPSHOT_ERROR);
      } else {
        setContent(snapshot);
      }
    } catch {
      setContent(null);
      setError(SNAPSHOT_ERROR);
    } finally {
      loadingRef.current = false;
      setLoading(false);
    }
  }

  /** 展开单项时懒读取；收起已读取内容时保留本轮本地预览。 */
  function toggle(): void {
    if (expanded) {
      setExpanded(false);
      return;
    }
    setExpanded(true);
    if (!content && !loadingRef.current) void load();
  }

  /** 对读取或校验失败的快照执行一次显式重试。 */
  function retry(): void {
    if (!loadingRef.current) void load();
  }

  return { expanded, loading, content, error, toggle, retry };
}
