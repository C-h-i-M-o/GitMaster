import { useRef, useState, type ChangeEvent } from "react";
import { agentApi, agentError } from "../services/agent";
import type { AgentMemory } from "../types/agentMemory";
import { memorySuggestionRequest } from "../ui/agentMemory";

/** 仅在用户确认后保存模型提出的工作区或用户级记忆建议。 */
export function useAgentMemorySuggestion({
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
  const [scope, setScope] = useState<"workspace" | "user">("workspace");
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const inFlight = useRef(false);

  /** 用户明确选择保存后绑定建议事件序号，避免重复保存。 */
  async function save(): Promise<AgentMemory | null> {
    if (inFlight.current || saved) return null;
    inFlight.current = true;
    setSaving(true);
    setError(null);
    try {
      const entry = await agentApi.saveMemory(
        memorySuggestionRequest(
          chatId,
          workspaceId,
          sequence,
          text,
          tags,
          scope,
        ),
      );
      setSaved(true);
      onSaved?.();
      return entry;
    } catch (cause) {
      setError(agentError(cause));
      return null;
    } finally {
      inFlight.current = false;
      setSaving(false);
    }
  }

  /** 更新用户主动选择的保存范围。 */
  function changeScope(userScope: boolean): void {
    if (saved || saving) return;
    setScope(userScope ? "user" : "workspace");
  }

  /** 把范围复选框转换为显式的保存范围选择。 */
  function scopeChanged(event: ChangeEvent<HTMLInputElement>): void {
    changeScope(event.target.checked);
  }

  /** 供按钮直接调用并由 hook 接管保存状态。 */
  function saveClicked(): void {
    void save();
  }

  return { scope, saving, saved, error, save, saveClicked, scopeChanged };
}
