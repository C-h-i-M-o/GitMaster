import { useNativeDialog } from "../hooks/useNativeDialog";
import { useAgentMemory } from "../hooks/useAgentMemory";
import type { ChatSession } from "../types/agent";
import { memoryExclusionReason } from "../ui/agentMemory";
import "./agentMemory.css";

/** 展示工作区记忆管理、选择预览与当前聊天的明确使用开关。 */
export function AgentMemoryPanel({
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
  const memory = useAgentMemory({
    enabled,
    workspaceId,
    chat,
    running,
    ensureChat,
    onChatUpdated,
    onMemoriesChanged,
  });
  const dialog = useNativeDialog(memory.open, memory.close);
  return (
    <>
      <button type="button" className="secondary" onClick={memory.show}>
        上下文与记忆
      </button>
      <dialog
        ref={dialog.dialogRef}
        onCancel={dialog.onCancel}
        className="gm-dialog agent-memory-dialog"
        aria-labelledby="agent-memory-title"
      >
        <header className="drawer-header">
          <div>
            <h2 id="agent-memory-title">上下文与记忆</h2>
            <small>当前工作区：{workspaceId}</small>
          </div>
          <button
            type="button"
            onClick={memory.close}
            aria-label="关闭记忆管理"
          >
            ×
          </button>
        </header>
        <div className="agent-memory-body">
          <section className="agent-memory-chat-config">
            <h3>本聊天记忆</h3>
            {!chat && (
              <p className="muted">
                设置会保存到此工作区的新聊天。保存时会先创建空聊天，不会发送消息。
              </p>
            )}
            <label>
              <input
                type="checkbox"
                checked={memory.config.enabled}
                onChange={memory.enabledChanged}
                disabled={memory.chatConfigLocked || memory.busy}
              />
              使用启用且匹配的记忆
            </label>
            <label>
              <input
                type="checkbox"
                checked={memory.config.includeUser}
                onChange={memory.includeUserChanged}
                disabled={memory.chatConfigLocked || memory.busy}
              />
              明确允许加入用户级跨工作区偏好
            </label>
            {memory.configDirty && !chat?.archived && (
              <button
                type="button"
                className="secondary"
                onClick={() => void memory.saveChatConfig()}
                disabled={memory.chatConfigLocked || memory.busy || !enabled}
              >
                {memory.busy
                  ? "保存中…"
                  : chat
                    ? "保存本聊天记忆设置"
                    : "创建空聊天并保存设置"}
              </button>
            )}
            {chat?.archived ? (
              <small role="status">
                归档聊天的记忆设置只读；你仍可管理工作区记忆。
              </small>
            ) : running ? (
              <small role="status">运行期间不能修改本聊天记忆设置。</small>
            ) : null}
          </section>

          <section className="agent-memory-preview">
            <h3>下次请求选择预览</h3>
            <label>
              检索提示
              <input
                value={memory.previewQuery}
                onChange={memory.previewQueryChanged}
                maxLength={200}
                placeholder="可选，最多 200 个字符"
              />
            </label>
            {!memory.config.enabled ? (
              <p className="muted">本聊天已关闭记忆，预览不会触发模型请求。</p>
            ) : memory.previewReady && memory.selection ? (
              <>
                <p>
                  预览估算{" "}
                  {memory.selection.estimatedTokens.toLocaleString("zh-CN")}{" "}
                  tokens；运行时会按真实预算重新检索。
                </p>
                {memory.selection.selected.map((entry) => (
                  <p className="agent-memory-preview-item" key={entry.id}>
                    将使用：{entry.text}
                  </p>
                ))}
                {memory.selection.excluded.map((item) => (
                  <p className="muted" key={item.id}>
                    {item.id}：{memoryExclusionReason(item.reason)}
                  </p>
                ))}
              </>
            ) : (
              <p className="muted">正在生成本地预览…</p>
            )}
          </section>

          <section className="agent-memory-list-section">
            <div className="agent-memory-heading">
              <h3>手工记忆</h3>
              <button
                type="button"
                className="secondary"
                onClick={memory.addEntry}
                disabled={running || memory.busy}
              >
                新增记忆
              </button>
            </div>
            <p className="muted">
              记忆仅由你手工保存；删除记忆不会更改历史对话。
            </p>
            {memory.loading && <p role="status">正在读取记忆…</p>}
            {memory.entries.map((entry) => (
              <article className="agent-memory-entry" key={entry.id}>
                <div>
                  <strong>{entry.text}</strong>
                  <small>
                    {entry.scope === "user"
                      ? "用户级跨工作区偏好"
                      : "当前工作区"}{" "}
                    · {entry.enabled ? "已启用" : "已停用"}
                  </small>
                  <small>
                    标签：{entry.tags.length ? entry.tags.join("、") : "无"}
                  </small>
                  <small>
                    来源：{entry.source.chatId ?? "未指定聊天"}
                    {entry.source.sequence === null
                      ? ""
                      : ` · 消息 ${entry.source.sequence}`}
                  </small>
                </div>
                <label className="agent-memory-exclude">
                  <input
                    type="checkbox"
                    checked={memory.config.excludedIds.includes(entry.id)}
                    onChange={memory.exclusionChanged(entry)}
                    disabled={running || memory.busy || !memory.config.enabled}
                  />
                  本聊天排除
                </label>
                <div className="agent-memory-entry-actions">
                  <button
                    type="button"
                    className="secondary"
                    onClick={() => void memory.toggleEntry(entry)}
                    disabled={running || memory.busy}
                  >
                    {entry.enabled ? "停用" : "启用"}
                  </button>
                  <button
                    type="button"
                    className="secondary"
                    onClick={() => memory.editEntry(entry)}
                    disabled={running || memory.busy}
                  >
                    编辑
                  </button>
                  {memory.deletePending === entry.id ? (
                    <>
                      <button
                        type="button"
                        className="secondary"
                        onClick={memory.keepMemory}
                      >
                        保留
                      </button>
                      <button
                        type="button"
                        className="danger-text"
                        onClick={() => void memory.deleteEntry()}
                        disabled={running || memory.busy}
                      >
                        确认删除
                      </button>
                    </>
                  ) : (
                    <button
                      type="button"
                      className="danger-text"
                      onClick={() => memory.askDelete(entry.id)}
                      disabled={running || memory.busy}
                    >
                      删除
                    </button>
                  )}
                </div>
              </article>
            ))}
            {!memory.loading && memory.entries.length === 0 && (
              <p className="muted">当前范围还没有手工记忆。</p>
            )}
          </section>

          {memory.entryDraft && (
            <form
              className="agent-memory-editor"
              onSubmit={(event) => {
                event.preventDefault();
                void memory.saveEntry();
              }}
            >
              <h3>{memory.entryDraft.id ? "编辑记忆" : "新增记忆"}</h3>
              <label>
                记忆内容
                <textarea
                  value={memory.entryDraft.text}
                  onChange={memory.textChanged}
                  rows={4}
                  required
                />
              </label>
              <label>
                标签
                <input
                  value={memory.tagsText}
                  onChange={memory.tagsChanged}
                  placeholder="例如：格式, 项目偏好"
                />
              </label>
              <label>
                <input
                  type="checkbox"
                  checked={memory.entryDraft.scope === "user"}
                  onChange={memory.scopeChanged}
                  disabled={running || memory.busy}
                />
                保存为用户级跨工作区偏好
              </label>
              <label>
                <input
                  type="checkbox"
                  checked={memory.entryDraft.source.chatId !== null}
                  onChange={memory.sourceChanged}
                  disabled={running || memory.busy || !chat}
                />
                记录当前聊天来源
              </label>
              {memory.entryDraft.source.chatId && (
                <label>
                  来源消息序号（可选）
                  <input
                    type="number"
                    min={1}
                    value={memory.entryDraft.source.sequence ?? ""}
                    onChange={memory.sourceSequenceChanged}
                    disabled={running || memory.busy}
                  />
                </label>
              )}
              {memory.entryDraft.scope === "user" && (
                <small>此条目会在明确允许的其他工作区聊天中可用。</small>
              )}
              <div className="agent-memory-entry-actions">
                <button
                  type="button"
                  className="secondary"
                  onClick={memory.cancelEntryEdit}
                  disabled={memory.busy}
                >
                  取消
                </button>
                <button
                  type="submit"
                  disabled={running || memory.busy || !enabled}
                >
                  {memory.busy ? "保存中…" : "保存记忆"}
                </button>
              </div>
            </form>
          )}
          {(memory.error || memory.notice) && (
            <p role={memory.error ? "alert" : "status"}>
              {memory.error ?? memory.notice}
            </p>
          )}
        </div>
      </dialog>
    </>
  );
}
