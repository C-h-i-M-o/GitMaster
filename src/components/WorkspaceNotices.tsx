import type { Workbench } from "../hooks/useWorkbench";
import { useWorkspaceNotices } from "../hooks/useWorkspaceNotices";

const labels = {
  progress: "进行中",
  success: "成功",
  warning: "需要留意",
  error: "失败",
};
const symbols = { progress: "◌", success: "✓", warning: "!", error: "×" };
/** 首页只呈现仓库级结果，详情按需展开，局部文件错误留在文件页。 */
export function WorkspaceNotices({ w }: { w: Workbench }) {
  const notices = useWorkspaceNotices(w);
  return (
    <div className="workspace-alerts" aria-live="polite">
      {notices.map((notice) => (
        <div
          key={notice.id}
          className={`workspace-notice ${notice.tone}`}
          role={
            notice.tone === "error" || notice.tone === "warning"
              ? "alert"
              : "status"
          }
        >
          <span className="notice-symbol" aria-label={labels[notice.tone]}>
            {symbols[notice.tone]}
          </span>
          <div className="notice-body">
            <span>{notice.message}</span>
            {notice.detail && (
              <details>
                <summary>查看详情</summary>
                <p>{notice.detail}</p>
              </details>
            )}
          </div>
          {notice.action === "remote" && (
            <button onClick={w.openModal("remote")}>打开合并流程</button>
          )}
          {notice.action === "resume" && (
            <button onClick={w.operations.resume}>重新查询任务</button>
          )}
        </div>
      ))}
    </div>
  );
}
