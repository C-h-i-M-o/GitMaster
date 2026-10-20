import type { Workbench } from "../hooks/useWorkbench";
import { UnifiedDiff } from "./UnifiedDiff";
import { PagedDiff } from "./PagedDiff";
import { ContentPreview } from "./ContentPreview";
import { Icon } from "./Icon";
import { describeGitError } from "../ui/gitPresentation";

/** 二级差异面板保留原文件列表与勾选状态，并明确展示比较侧。 */
export function ChangeDetailDrawer({ workbench: w }: { workbench: Workbench }) {
  if (w.drawer !== "changes" || !w.changeDetailOpen || !w.repo.selected)
    return null;
  const change = w.repo.repository?.changes.find(
    (item) => item.changeId === w.repo.selected?.changeId,
  );
  return (
    <aside
      ref={w.changeDetailFocus.detail}
      onKeyDown={w.changeDetailFocus.keyDown}
      className="change-detail-drawer"
      aria-label="文件详细变化"
    >
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
        {change?.path ?? w.repo.selected.path}
      </div>
      <div className="change-detail-body">
        {w.repo.error ? (
          <div className="content-preview change-error" role="alert">
            <p>{describeGitError(w.repo.error)}</p>
            <button className="secondary" onClick={w.refreshLocal}>
              重试刷新
            </button>
          </div>
        ) : null}
        {w.repo.selected.side !== "untracked" &&
          ((w.repo.diff?.kind === "text" && w.repo.diff.content) ||
            w.repo.diff?.kind === "paged") && (
            <p className="diff-context-note">
              {w.repo.diff.truncated
                ? "内容受安全上限限制，已截断。"
                : "已展示完整差异上下文。"}
            </p>
          )}
        {w.repo.error ? null : !w.repo.diffLoading &&
          w.repo.diff?.kind === "paged" &&
          w.repo.repository ? (
          <PagedDiff
            key={w.repo.diff.documentId}
            document={w.repo.diff}
            scope={{
              repositoryId: w.repo.repository.repositoryId,
              snapshotId: w.repo.repository.snapshotId,
            }}
          />
        ) : !w.repo.diffLoading && w.repo.diff?.kind === "text" ? (
          <UnifiedDiff
            content={w.repo.diff.content}
            untracked={w.repo.selected.side === "untracked"}
            truncated={w.repo.diff.truncated}
          />
        ) : (
          <ContentPreview
            value={w.repo.diff?.kind === "paged" ? null : w.repo.diff}
            loading={w.repo.diffLoading}
          />
        )}
      </div>
    </aside>
  );
}
