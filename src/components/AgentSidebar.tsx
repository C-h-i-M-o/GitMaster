import type { AgentController } from "../hooks/useAgent";
import type { ChatSession } from "../types/agent";
import { chatMenuDisabledReason } from "../ui/agentPresentation";
import "./agentModels.css";
import { AgentChatSearch } from "./AgentChatSearch";
import "./agentMemory.css";

/** 显示一条聊天及其受运行状态保护的操作菜单。 */
function AgentChatRow({ a, chat }: { a: AgentController; chat: ChatSession }) {
  const disabledReason = chatMenuDisabledReason(
    chat.id,
    a.record?.session.id ?? null,
    a.run?.runId ?? null,
    a.busy,
  );
  const operationDisabled = disabledReason !== null;
  return (
    <div className="agent-chat-row">
      <button
        className={chat.id === a.record?.session.id ? "active" : ""}
        onClick={a.select(chat)}
        disabled={a.busy}
      >
        <strong>{chat.title}</strong>
        <small title={chat.repositoryPath ?? ""}>
          {a.chatWorkspaceAvailable(chat)
            ? (chat.repositoryPath?.split(/[\\/]/).at(-1) ?? "默认工作区")
            : "工作区不可用 · 仅可阅读"}
        </small>
      </button>
      <details className="agent-chat-menu">
        <summary aria-label={`管理聊天：${chat.title}`}>⋯</summary>
        <div className="agent-chat-menu-content">
          {disabledReason && (
            <small className="agent-chat-disabled-reason" role="status">
              {disabledReason}
            </small>
          )}
          {a.renameTarget === chat.id ? (
            <div className="agent-chat-rename">
              <label>
                聊天名称
                <input
                  value={a.title}
                  onChange={a.changeTitle}
                  aria-invalid={!!a.titleValidation.error}
                  disabled={operationDisabled}
                />
              </label>
              {a.titleValidation.error && (
                <small role="status">{a.titleValidation.error}</small>
              )}
              <button
                className="secondary"
                onClick={a.rename}
                disabled={operationDisabled || !!a.titleValidation.error}
              >
                保存名称
              </button>
              <button className="secondary" onClick={a.cancelRename}>
                取消
              </button>
            </div>
          ) : (
            <button
              className="secondary"
              onClick={a.beginRename(chat)}
              disabled={operationDisabled}
              title={disabledReason ?? undefined}
            >
              重命名
            </button>
          )}
          <button
            className="secondary"
            onClick={a.archive(chat)}
            disabled={operationDisabled}
            title={disabledReason ?? undefined}
          >
            {chat.archived ? "恢复聊天" : "归档聊天"}
          </button>
          {a.deletePending === chat.id ? (
            <div className="agent-delete-confirm">
              <p>永久删除此聊天的本地记录？此操作无法撤销。</p>
              <button className="secondary" onClick={a.keepChat}>
                保留
              </button>
              <button
                className="danger-text"
                onClick={a.deleteChat}
                disabled={operationDisabled}
                title={disabledReason ?? undefined}
              >
                确认删除
              </button>
            </div>
          ) : (
            <button
              className="danger-text"
              onClick={a.askDelete(chat)}
              disabled={operationDisabled}
              title={disabledReason ?? undefined}
            >
              删除聊天
            </button>
          )}
        </div>
      </details>
    </div>
  );
}

/** 在应用共用的左侧栏中列出 AI 聊天和聊天操作。 */
export function AgentSidebar({
  a,
  repositoryPath,
}: {
  a: AgentController;
  repositoryPath: string | null;
}) {
  return (
    <section className="agent-sidebar" aria-label="AI 聊天列表">
      <header className="agent-sidebar-heading">
        <strong>AI Agent</strong>
        <button className="secondary" onClick={a.create} disabled={a.busy}>
          新对话
        </button>
      </header>
      <small
        className="agent-sidebar-path"
        title={a.selectedWorkspace?.rootPath ?? repositoryPath ?? ""}
      >
        {a.selectedWorkspace?.label ??
          (repositoryPath ? "正在读取 AI 工作区" : "默认工作区")}
      </small>
      <label className="agent-workspace-picker">
        新对话工作区
        <select
          aria-label="新对话工作区"
          value={a.workspaceId}
          onChange={a.selectWorkspace}
          disabled={a.workspaceBusy}
        >
          {a.workspaces.map((workspace) => (
            <option key={workspace.id} value={workspace.id}>
              {workspace.label}
            </option>
          ))}
        </select>
      </label>
      {a.selectedWorkspace && (
        <div className="agent-workspace-paths">
          <small title={a.selectedWorkspace.rootPath}>
            根目录：{a.selectedWorkspace.rootPath}
          </small>
          <small title={a.selectedWorkspace.writePath}>
            默认输出：{a.selectedWorkspace.writePath}
          </small>
          <small>
            {a.selectedWorkspace.gitAvailable ? "可用 Git" : "无 Git 仓库"}
          </small>
        </div>
      )}
      <button
        className="secondary agent-choose-folder"
        onClick={a.chooseFolder}
        disabled={a.workspaceBusy}
      >
        {a.workspaceBusy ? "正在选择…" : "选择项目文件夹"}
      </button>
      <AgentChatSearch search={a.search} onSelect={a.selectSearchResult} />
      <button
        className="agent-archive-filter"
        aria-pressed={a.archived}
        onClick={a.toggleArchived}
      >
        {a.archived ? "正在查看归档 · 查看聊天" : "聊天记录 · 查看归档"}
      </button>
      <div className="agent-history-list">
        {a.chats.map((chat) => (
          <AgentChatRow a={a} chat={chat} key={chat.id} />
        ))}
      </div>
      {a.unavailable.length > 0 && (
        <p role="alert">
          有 {a.unavailable.length} 个聊天无法读取，原文件已保留。
        </p>
      )}
    </section>
  );
}
