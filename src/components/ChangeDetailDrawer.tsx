import type { Workbench } from "../hooks/useWorkbench";
import { UnifiedDiff } from "./UnifiedDiff";
import { ContentPreview } from "./ContentPreview";
import { Icon } from "./Icon";

/** 二级差异面板保留原文件列表与勾选状态，并明确展示比较侧。 */
export function ChangeDetailDrawer({ workbench: w }: { workbench: Workbench }) {
  if (w.drawer !== "changes" || !w.changeDetailOpen || !w.repo.selected)
    return null;
  const change = w.repo.repository?.changes.find(
    (item) => item.changeId === w.repo.selected?.changeId,
  );
  return (
    <aside className="change-detail-drawer" aria-label="文件详细变化">
      <header className="drawer-header">
        <div>
          <h2>文件变化</h2>
          <small>
            {w.repo.selected.side === "staged"
              ? "已暂存 · HEAD → 暂存区"
              : w.repo.selected.side === "unstaged"
                ? "未暂存 · 暂存区 → 工作区"
                : "未跟踪 · 新增内容"}
          </small>
        </div>
        <button
          className="icon-button"
          onClick={w.closeChangeDetail}
          aria-label="返回本地修改"
        >
          <Icon name="close" />
        </button>
      </header>
      <div className="change-detail-path" title={change?.path}>
        {change?.originalPath && <span>{change.originalPath} → </span>}
        {change?.path}
      </div>
      <div className="change-detail-body">
        {w.repo.selected.side !== "untracked" &&
          w.repo.diff?.kind === "text" && (
            <div className="button-row">
              <button
                className="secondary"
                onClick={w.expandChangeContext}
                disabled={
                  w.repo.diffLoading ||
                  w.repo.diff.truncated ||
                  (w.repo.selected.contextLines ?? 3) >= 2000
                }
              >
                加载更多上下文
              </button>
              <small className="muted">
                每处变化前后最多 {w.repo.selected.contextLines ?? 3}{" "}
                行；受内容加载上限限制
              </small>
            </div>
          )}
        {!w.repo.diffLoading && w.repo.diff?.kind === "text" ? (
          <UnifiedDiff
            content={w.repo.diff.content}
            untracked={w.repo.selected.side === "untracked"}
            truncated={w.repo.diff.truncated}
          />
        ) : (
          <ContentPreview value={w.repo.diff} loading={w.repo.diffLoading} />
        )}
      </div>
    </aside>
  );
}
