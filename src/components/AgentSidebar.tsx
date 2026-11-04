import type { AgentController } from "../hooks/useAgent";

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
      <small className="agent-sidebar-path" title={repositoryPath ?? ""}>
        {repositoryPath?.split(/[\\/]/).at(-1) ?? "默认工作区"}
      </small>
      <button
        className="agent-archive-filter"
        aria-pressed={a.archived}
        onClick={a.toggleArchived}
      >
        {a.archived ? "正在查看归档 · 查看聊天" : "聊天记录 · 查看归档"}
      </button>
      <div className="agent-history-list">
        {a.chats.map((chat) => (
          <div className="agent-chat-row" key={chat.id}>
            <button
              className={chat.id === a.record?.session.id ? "active" : ""}
              onClick={a.select(chat)}
              disabled={a.busy}
            >
              <strong>{chat.title}</strong>
              <small title={chat.repositoryPath ?? ""}>
                {chat.repositoryPath?.split(/[\\/]/).at(-1) ?? "无项目"}
              </small>
            </button>
            <details className="agent-chat-menu">
              <summary aria-label={`管理聊天：${chat.title}`}>⋯</summary>
              <div className="agent-chat-menu-content">
                {a.renameTarget === chat.id ? (
                  <div className="agent-chat-rename">
                    <label>
                      聊天名称
                      <input
                        value={a.title}
                        onChange={a.changeTitle}
                        aria-invalid={!!a.titleValidation.error}
                        disabled={a.busy}
                      />
                    </label>
                    {a.titleValidation.error && (
                      <small role="status">{a.titleValidation.error}</small>
                    )}
                    <button
                      className="secondary"
                      onClick={a.rename}
                      disabled={a.busy || !!a.titleValidation.error}
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
                    disabled={
                      a.busy || (!!a.run && chat.id === a.record?.session.id)
                    }
                  >
                    重命名
                  </button>
                )}
                <button
                  className="secondary"
                  onClick={a.archive(chat)}
                  disabled={
                    a.busy || (!!a.run && chat.id === a.record?.session.id)
                  }
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
                      disabled={a.busy}
                    >
                      确认删除
                    </button>
                  </div>
                ) : (
                  <button
                    className="danger-text"
                    onClick={a.askDelete(chat)}
                    disabled={
                      a.busy || (!!a.run && chat.id === a.record?.session.id)
                    }
                  >
                    删除聊天
                  </button>
                )}
              </div>
            </details>
          </div>
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
