import { useUnifiedDiff } from "../hooks/useUnifiedDiff";

/** 以旧新行号和增删色展示只读统一差异，内容始终作为文本渲染。 */
export function UnifiedDiff({
  content,
  untracked,
  truncated,
}: {
  content: string;
  untracked: boolean;
  truncated: boolean;
}) {
  const { diff, rows, toggle } = useUnifiedDiff(content, untracked);
  return (
    <div className="unified-diff">
      <div className="diff-summary">
        {truncated ? "已加载部分 · " : ""}
        <span className="diff-added">+{diff.additions}</span>
        <span className="diff-deleted">−{diff.deletions}</span>
      </div>
      {truncated && <p role="status">内容已截断，统计仅包括已加载部分。</p>}
      {diff.lines.length ? (
        <div className="diff-lines" tabIndex={0} aria-label="只读统一差异">
          {rows.map((row) =>
            row.kind === "fold" ? (
              <button
                className="diff-fold"
                key={`fold-${row.id}`}
                aria-expanded={row.expanded}
                onClick={toggle(row.id)}
              >
                {row.expanded ? "收起" : "展开"} {row.count} 行未更改内容
              </button>
            ) : (
              <div className={`diff-line ${row.line.kind}`} key={row.index}>
                <span className="diff-number" aria-label="旧行号">
                  {row.line.oldLine}
                </span>
                <span className="diff-number" aria-label="新行号">
                  {row.line.newLine}
                </span>
                <span className="diff-sign" aria-hidden="true">
                  {row.line.kind === "add"
                    ? "+"
                    : row.line.kind === "delete"
                      ? "−"
                      : ""}
                </span>
                <code>{row.line.text || " "}</code>
              </div>
            ),
          )}
        </div>
      ) : (
        <p className="muted">
          {content
            ? "没有文本行变化（可能仅文件名或权限变化）。"
            : "空文件或没有文本差异。"}
        </p>
      )}
    </div>
  );
}
