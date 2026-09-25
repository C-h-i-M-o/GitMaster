import { usePagedDiff, type PagedDiffProps } from "../hooks/usePagedDiff";
import { describeGitError } from "../ui/gitPresentation";

/** 只挂载可见差异行，正文始终作为文本展示。 */
export function PagedDiff(props: PagedDiffProps) {
  const view = usePagedDiff(props);
  return (
    <section className="unified-diff" aria-label="分块统一差异">
      <div className="diff-summary">
        {props.document.truncated ? "已加载部分 · " : ""}
        <span className="diff-added">+{props.document.additions}</span>
        <span className="diff-deleted">−{props.document.deletions}</span>
        <span>{props.document.hunkCount} 处变化</span>
      </div>
      {props.document.truncated && (
        <p role="status">内容超出加载上限，统计仅包括已加载部分。</p>
      )}
      {view.error && (
        <p role="alert">{describeGitError(view.error)} 请重新选择文件读取。</p>
      )}
      {props.document.rowCount === 0 ? (
        <p className="muted">没有文本行变化（空文件、文件名或权限变化）。</p>
      ) : (
        <div
          ref={view.container}
          className="diff-lines paged-diff-viewport"
          tabIndex={0}
          aria-label="只读差异，可选择复制当前文本"
          aria-busy={view.busy}
        >
          <div
            style={{
              height: view.totalSize,
              position: "relative",
              minWidth: "100%",
              width: "max-content",
            }}
          >
            {view.items.map((item) => {
              const position = view.positions[item.index];
              if (!position) return null;
              if (position.kind === "fold")
                return (
                  <button
                    key={item.key}
                    className="diff-fold paged-diff-fold"
                    aria-expanded={position.expanded}
                    onClick={view.toggle(position.source)}
                    style={{
                      position: "absolute",
                      top: 0,
                      left: 0,
                      height: item.size,
                      transform: `translateY(${item.start}px)`,
                    }}
                  >
                    {position.expanded ? "收起" : "展开"} {position.count}{" "}
                    行未更改内容
                  </button>
                );
              const row = view.row(position.source);
              return (
                <div
                  key={item.key}
                  className={`diff-line ${row?.kind ?? ""}`}
                  data-row={position.source}
                  style={{
                    position: "absolute",
                    top: 0,
                    left: 0,
                    height: item.size,
                    transform: `translateY(${item.start}px)`,
                  }}
                >
                  <span className="diff-number" aria-label="旧行号">
                    {row?.oldLine}
                  </span>
                  <span className="diff-number" aria-label="新行号">
                    {row?.newLine}
                  </span>
                  <span className="diff-sign" aria-hidden="true">
                    {row?.kind === "add"
                      ? "+"
                      : row?.kind === "delete"
                        ? "−"
                        : ""}
                  </span>
                  <code>
                    {row
                      ? row.kind === "note"
                        ? "文件末尾没有换行符"
                        : row.text || " "
                      : view.error
                        ? "（读取已停止）"
                        : "（正在读取）"}
                  </code>
                  {row &&
                    (row.byteOffset > 0 || row.nextByteOffset !== null) && (
                      <span className="reader-segment">
                        <span>分段 · 字节偏移 {row.byteOffset}</span>
                        {row.byteOffset > 0 && (
                          <button
                            disabled={view.busy}
                            onClick={view.segment(position.source, 0)}
                          >
                            首段
                          </button>
                        )}
                        {row.nextByteOffset !== null && (
                          <button
                            disabled={view.busy}
                            onClick={view.segment(
                              position.source,
                              row.nextByteOffset,
                            )}
                          >
                            下一段
                          </button>
                        )}
                      </span>
                    )}
                </div>
              );
            })}
          </div>
        </div>
      )}
    </section>
  );
}
