import type { AiWorkspaceController } from "../../hooks/useAiWorkspace";
import { useAiTheme } from "../../hooks/useAiTheme";
import { Icon } from "../Icon";
import "./upstream-theme.css";
import "./ai.css";

/** AI 模式复用项目左栏展示会话，返回按钮不结束任务或改变 Git 视图。 */
export function AiSessionSidebar({
  f,
  onReturn,
}: {
  f: AiWorkspaceController;
  onReturn: () => void;
}) {
  const dark = useAiTheme();
  return (
    <nav
      className="ai-surface ai-session-list"
      data-ai-dark={dark ? "" : undefined}
      aria-label="AI 项目导航"
    >
      <button
        className="ai-new-session"
        onClick={f.create}
        disabled={!f.validModel || !f.repositoryId || f.busy || f.preview}
      >
        <Icon name="plus" />
        新会话
      </button>
      <p className="ai-list-heading">此项目的会话</p>
      <div className="ai-session-items">
        {f.sessions.map((item) => (
          <div
            className={`ai-session-item ${item.sessionId === f.session?.sessionId ? "selected" : ""}`}
            key={item.sessionId}
          >
            <button onClick={f.select(item)} title={item.title}>
              {item.title}
            </button>
            <button aria-label={`删除${item.title}`} onClick={f.remove(item)}>
              <Icon name="close" />
            </button>
          </div>
        ))}
        {f.nextOffset !== null && <button onClick={f.more}>加载更多</button>}
      </div>
      <button className="ai-return-git" onClick={onReturn}>
        <Icon name="history" />
        返回 Git 工作台
      </button>
    </nav>
  );
}
