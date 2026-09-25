import { usePagedReader, type PagedReaderProps } from "../hooks/usePagedReader";
import { describeGitError } from "../ui/gitPresentation";
/** 有界只读视图：普通文本直接渲染，不执行仓库中的标记或链接。 */
export function PagedReader(props: PagedReaderProps) {
  const view = usePagedReader(props);
  return (
    <section className="paged-reader" aria-label="分块只读文件">
      <div className="button-row">
        <span>
          {view.document
            ? `只读 · UTF-8${view.document.bom ? " BOM" : ""} · ${view.document.lineEnding} · ${view.document.lineCount} 行`
            : "分块只读"}
        </span>
        <button onClick={view.reload} disabled={view.loading}>
          重新读取
        </button>
      </div>
      <div className="reader-tools">
        <form onSubmit={view.jump}>
          <label>
            行号{" "}
            <input
              aria-label="跳转行号"
              inputMode="numeric"
              value={view.lineInput}
              onChange={view.changeLine}
            />
          </label>
          <button disabled={!view.document}>跳转</button>
        </form>
        <form onSubmit={view.find}>
          <input
            aria-label="文件内查找"
            placeholder="区分大小写"
            maxLength={1024}
            value={view.query}
            onChange={view.changeQuery}
          />
          <button disabled={!view.document || view.searching}>
            查找下一个
          </button>
        </form>
      </div>
      {view.error && <p role="alert">{describeGitError(view.error)}</p>}
      {view.loading && <p role="status">正在建立行索引…</p>}
      {view.notice && <p role="status">{view.notice}</p>}
      <div
        ref={view.container}
        className="reader-viewport"
        tabIndex={0}
        aria-label="只读正文，可选择并复制当前文本"
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
            const line = view.line(item.index);
            return (
              <div
                key={item.key}
                className="reader-line"
                data-line={item.index + 1}
                style={{
                  position: "absolute",
                  top: 0,
                  left: 0,
                  height: item.size,
                  transform: `translateY(${item.start}px)`,
                }}
              >
                <span
                  className="reader-line-number"
                  aria-label={`第 ${item.index + 1} 行`}
                >
                  {item.index + 1}
                </span>
                <span className="reader-text">
                  {line
                    ? line.text
                    : view.error
                      ? "（读取已停止）"
                      : "（正在读取）"}
                </span>
                {line &&
                  (line.byteOffset > 0 || line.nextByteOffset !== null) && (
                    <span className="reader-segment">
                      <span>分段 · 字节偏移 {line.byteOffset}</span>
                      {line.byteOffset > 0 && (
                        <button
                          disabled={view.busy}
                          onClick={view.segment(item.index, 0)}
                        >
                          首段
                        </button>
                      )}
                      {line.nextByteOffset !== null && (
                        <button
                          disabled={view.busy}
                          onClick={view.segment(
                            item.index,
                            line.nextByteOffset,
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
    </section>
  );
}
