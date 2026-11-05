import { useEffect, useRef, useState, type ChangeEvent } from "react";
import { agentApi, agentError } from "../services/agent";
import type { ChatSession } from "../types/agent";
import type {
  AgentMemory,
  ChatMemoryConfig,
  MemorySelection,
  SaveMemory,
} from "../types/agentMemory";
import {
  chatMemoryMatchesSession,
  defaultChatMemoryConfig,
  memoryEditRequest,
  parseMemoryTags,
} from "../ui/agentMemory";

/** 管理 workspace/user 记忆、显式选择预览和单聊天记忆配置。 */
export function useAgentMemory({
  enabled,
  workspaceId,
  chat,
  running,
  ensureChat,
  onChatUpdated,
  onMemoriesChanged,
}: {
  enabled: boolean;
  workspaceId: string;
  chat: ChatSession | null;
  running: boolean;
  ensureChat: () => Promise<ChatSession>;
  onChatUpdated: (session: ChatSession) => void;
  onMemoriesChanged?: () => void;
}) {
  const [open, setOpen] = useState(false);
  const [entries, setEntries] = useState<AgentMemory[]>([]);
  const [entriesScope, setEntriesScope] = useState("");
  const [loading, setLoading] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [selection, setSelection] = useState<MemorySelection | null>(null);
  const [selectionScope, setSelectionScope] = useState("");
  const [previewQuery, setPreviewQuery] = useState("");
  const [tagsText, setTagsText] = useState("");
  const [config, setConfig] = useState<ChatMemoryConfig>(
    chat?.memory ?? defaultChatMemoryConfig(),
  );
  const [entryDraft, setEntryDraft] = useState<SaveMemory | null>(null);
  const [deletePending, setDeletePending] = useState<string | null>(null);
  const listGeneration = useRef(0);
  const selectionGeneration = useRef(0);
  const configDrafts = useRef(new Map<string, ChatMemoryConfig>());
  const chatConfigLocked = running || chat?.archived === true;

  /** 在打开或切换作用域后重读用户明确可管理的所有记忆条目。 */
  useEffect(() => {
    if (!open) return;
    const request = ++listGeneration.current;
    const scope = workspaceId;
    setLoading(true);
    setEntriesScope("");
    setEntryDraft(null);
    setDeletePending(null);
    setError(null);
    void agentApi
      .listMemories(workspaceId, true)
      .then((items) => {
        if (listGeneration.current === request) {
          setEntries(items);
          setEntriesScope(scope);
        }
      })
      .catch((cause: unknown) => {
        if (listGeneration.current === request) setError(agentError(cause));
      })
      .finally(() => {
        if (listGeneration.current === request) setLoading(false);
      });
    return () => {
      listGeneration.current += 1;
    };
  }, [open, workspaceId]);

  /** 将每聊天配置从 session 快照或该工作区的未建聊天草稿恢复。 */
  useEffect(() => {
    const next = chat
      ? chat.memory
      : (configDrafts.current.get(workspaceId) ?? defaultChatMemoryConfig());
    setConfig({ ...next, excludedIds: [...next.excludedIds] });
  }, [
    workspaceId,
    chat?.id,
    chat?.memory.enabled,
    chat?.memory.includeUser,
    chat?.memory.excludedIds,
  ]);

  /** 以 2048 token 预算预览当前查询与显式排除列表的选择结果。 */
  useEffect(() => {
    const request = ++selectionGeneration.current;
    const query = previewQuery.trim();
    const scope = JSON.stringify([
      workspaceId,
      config.enabled,
      config.includeUser,
      config.excludedIds,
      query,
    ]);
    if (!open || !config.enabled || !enabled) {
      setSelection(null);
      setSelectionScope("");
      return () => {
        selectionGeneration.current += 1;
      };
    }
    setSelection(null);
    setSelectionScope("");
    const timer = window.setTimeout(() => {
      void agentApi
        .selectMemories(
          workspaceId,
          config.includeUser,
          query,
          config.excludedIds,
          2048,
        )
        .then((result) => {
          if (selectionGeneration.current === request) {
            setSelection(result);
            setSelectionScope(scope);
          }
        })
        .catch((cause: unknown) => {
          if (selectionGeneration.current === request)
            setError(agentError(cause));
        });
    }, 180);
    return () => {
      window.clearTimeout(timer);
      selectionGeneration.current += 1;
    };
  }, [
    open,
    enabled,
    workspaceId,
    config.enabled,
    config.includeUser,
    config.excludedIds,
    previewQuery,
  ]);

  /** 只在用户点击上下文与记忆入口后展开管理界面。 */
  function show(): void {
    setError(null);
    setNotice(null);
    setOpen(true);
  }

  /** 关闭面板不保存未提交条目或会话配置草稿。 */
  function close(): void {
    setOpen(false);
    setEntryDraft(null);
    setDeletePending(null);
  }

  /** 更新新聊天草稿或当前聊天待保存的记忆使用设置。 */
  function updateConfig(next: ChatMemoryConfig): void {
    const safe = { ...next, excludedIds: [...new Set(next.excludedIds)] };
    setConfig(safe);
    if (!chat) configDrafts.current.set(workspaceId, safe);
  }

  /** 新建一条当前工作区记忆，不调用模型自动写入。 */
  function addEntry(): void {
    setTagsText("");
    setEntryDraft({
      id: null,
      expectedRevision: null,
      scope: "workspace",
      workspaceId,
      text: "",
      tags: [],
      source: { chatId: null, sequence: null },
      enabled: true,
    });
  }

  /** 将条目复制到编辑草稿，不修改当前列表快照。 */
  function editEntry(entry: AgentMemory): void {
    setTagsText(entry.tags.join(", "));
    setEntryDraft(memoryEditRequest(entry));
  }

  /** 编辑中的文本字段只更新内存草稿。 */
  function textChanged(event: ChangeEvent<HTMLTextAreaElement>): void {
    setEntryDraft(
      (current) => current && { ...current, text: event.target.value },
    );
  }

  /** 编辑标签字段并清理空值与重复标签。 */
  function tagsChanged(event: ChangeEvent<HTMLInputElement>): void {
    setTagsText(event.target.value);
  }

  /** 跨工作区范围必须由用户主动勾选，避免意外共享偏好。 */
  function scopeChanged(event: ChangeEvent<HTMLInputElement>): void {
    const userScope = event.target.checked;
    setEntryDraft(
      (current) =>
        current && {
          ...current,
          scope: userScope ? "user" : "workspace",
          workspaceId: userScope ? null : workspaceId,
        },
    );
  }

  /** 只在用户明确勾选后将当前聊天作为记忆来源。 */
  function sourceChanged(event: ChangeEvent<HTMLInputElement>): void {
    setEntryDraft(
      (current) =>
        current && {
          ...current,
          source: {
            chatId: event.target.checked ? (chat?.id ?? null) : null,
            sequence: event.target.checked ? current.source.sequence : null,
          },
        },
    );
  }

  /** 手工更新可选来源消息序号，并保持来源聊天标识。 */
  function sourceSequenceChanged(event: ChangeEvent<HTMLInputElement>): void {
    const value = event.target.value.trim();
    const sequence = value ? Number(event.target.valueAsNumber) : null;
    if (sequence !== null && (!Number.isInteger(sequence) || sequence < 1))
      return;
    setEntryDraft(
      (current) =>
        current && {
          ...current,
          source: { ...current.source, sequence },
        },
    );
  }

  /** 保存或更新记忆条目，后续模型请求会重新选择记忆。 */
  async function saveEntry(): Promise<void> {
    if (!entryDraft || busy || running) return;
    if (!entryDraft.text.trim()) {
      setError("记忆内容不能为空。");
      return;
    }
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const saved = await agentApi.saveMemory({
        ...entryDraft,
        text: entryDraft.text.trim(),
        tags: parseMemoryTags(tagsText),
        source: { ...entryDraft.source },
      });
      setEntries((current) => [
        saved,
        ...current.filter((entry) => entry.id !== saved.id),
      ]);
      setEntryDraft(null);
      setTagsText("");
      setNotice("记忆已保存；下一次模型请求会应用更新。");
      onMemoriesChanged?.();
    } catch (cause) {
      setError(agentError(cause));
    } finally {
      setBusy(false);
    }
  }

  /** 启用或停用单条记忆，历史聊天内容不会改变。 */
  async function toggleEntry(entry: AgentMemory) {
    if (busy || running) return;
    setBusy(true);
    setError(null);
    try {
      const updated = await agentApi.saveMemory({
        ...memoryEditRequest(entry),
        enabled: !entry.enabled,
      });
      setEntries((current) =>
        current.map((item) => (item.id === updated.id ? updated : item)),
      );
      onMemoriesChanged?.();
    } catch (cause) {
      setError(agentError(cause));
    } finally {
      setBusy(false);
    }
  }

  /** 删除前记录目标 ID，确认后使用条目 revision 执行删除。 */
  function askDelete(id: string): void {
    setDeletePending(id);
  }

  /** 撤销记忆删除确认，不改变存储。 */
  function keepMemory(): void {
    setDeletePending(null);
  }

  /** 确认删除记忆，不改写任何既有聊天历史。 */
  async function deleteEntry(): Promise<void> {
    const entry = entries.find((item) => item.id === deletePending);
    if (!entry || busy || running) return;
    setBusy(true);
    setError(null);
    try {
      await agentApi.deleteMemory(entry.id, entry.revision);
      setEntries((current) => current.filter((item) => item.id !== entry.id));
      onMemoriesChanged?.();
      if (!chat?.archived)
        updateConfig({
          ...config,
          excludedIds: config.excludedIds.filter((id) => id !== entry.id),
        });
      setDeletePending(null);
      setNotice("记忆已删除，聊天历史保持不变。");
    } catch (cause) {
      setError(agentError(cause));
    } finally {
      setBusy(false);
    }
  }

  /** 按 200 字符上限更新记忆检索预览词。 */
  function previewQueryChanged(event: ChangeEvent<HTMLInputElement>): void {
    setPreviewQuery(Array.from(event.target.value).slice(0, 200).join(""));
  }

  /** 取消当前记忆编辑并清空尚未保存的表单状态。 */
  function cancelEntryEdit(): void {
    setEntryDraft(null);
    setTagsText("");
  }

  /** 在会话级别排除或恢复某条记忆，运行期间锁定配置。 */
  function exclusionChanged(entry: AgentMemory) {
    return (event: ChangeEvent<HTMLInputElement>): void => {
      if (chatConfigLocked) return;
      const excluded = new Set(config.excludedIds);
      if (event.target.checked) excluded.add(entry.id);
      else excluded.delete(entry.id);
      updateConfig({ ...config, excludedIds: [...excluded] });
    };
  }

  /** 独立开启或关闭当前聊天的记忆使用。 */
  function enabledChanged(event: ChangeEvent<HTMLInputElement>): void {
    if (!chatConfigLocked)
      updateConfig({ ...config, enabled: event.target.checked });
  }

  /** 用户跨工作区偏好必须再次由会话明确允许后才进入检索。 */
  function includeUserChanged(event: ChangeEvent<HTMLInputElement>): void {
    if (!chatConfigLocked)
      updateConfig({ ...config, includeUser: event.target.checked });
  }

  /** 保存聊天级记忆开关；尚未发送的新聊天会先创建空会话。 */
  async function saveChatConfig(): Promise<boolean> {
    if (chatConfigLocked || busy) return false;
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const session = chat ?? (await ensureChat());
      const updated = await agentApi.setChatMemory(session.id, config);
      onChatUpdated(updated);
      setConfig(updated.memory);
      configDrafts.current.delete(workspaceId);
      setNotice("本聊天记忆设置已保存。");
      return true;
    } catch (cause) {
      setError(agentError(cause));
      return false;
    } finally {
      setBusy(false);
    }
  }

  const visibleEntries = entriesScope === workspaceId ? entries : [];
  const previewScope = JSON.stringify([
    workspaceId,
    config.enabled,
    config.includeUser,
    config.excludedIds,
    previewQuery.trim(),
  ]);
  const defaultConfig = defaultChatMemoryConfig();
  const configDirty = chat
    ? !chatMemoryMatchesSession(config, chat)
    : config.enabled !== defaultConfig.enabled ||
      config.includeUser !== defaultConfig.includeUser ||
      config.excludedIds.length > 0;
  return {
    open,
    entries: visibleEntries,
    loading,
    busy,
    running,
    chatConfigLocked,
    error,
    notice,
    selection,
    previewReady: selectionScope === previewScope,
    previewQuery,
    tagsText,
    config,
    configDirty,
    entryDraft,
    deletePending,
    show,
    close,
    addEntry,
    editEntry,
    textChanged,
    tagsChanged,
    scopeChanged,
    sourceChanged,
    sourceSequenceChanged,
    saveEntry,
    toggleEntry,
    askDelete,
    keepMemory,
    deleteEntry,
    previewQueryChanged,
    exclusionChanged,
    enabledChanged,
    includeUserChanged,
    saveChatConfig,
    cancelEntryEdit,
  };
}
