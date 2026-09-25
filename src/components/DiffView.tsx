import type { DiffSide, FileChange } from "../types/git";
import type { LocalDiff, DiffScope } from "../types/diff";
import { PagedDiff } from "./PagedDiff";
import { describeUnsupported } from "../ui/gitPresentation";
interface Props {
  diff: LocalDiff | null;
  scope: DiffScope;
  loading: boolean;
  selected: { changeId: string; side: DiffSide } | null;
  change: FileChange | undefined;
}
/** 展示当前文件、比较侧和受限差异内容。 */
export function DiffView({ diff, loading, selected, change, scope }: Props) {
  return (
    <div className="diff-panel">
      <h3>文件内容</h3>
      {selected && (
        <p>
          {change?.originalPath ? `${change.originalPath} → ` : ""}
          {change?.path} ·{" "}
          {selected.side === "staged"
            ? "已暂存"
            : selected.side === "unstaged"
              ? "未暂存"
              : "未跟踪预览"}
        </p>
      )}
      {loading ? (
        <p>正在读取…</p>
      ) : diff?.kind === "paged" ? (
        <PagedDiff key={diff.documentId} document={diff} scope={scope} />
      ) : diff?.kind === "text" ? (
        <>
          {diff.truncated && (
            <p role="alert">内容已截断，仅显示前 5000 行或 1 MiB。</p>
          )}
          <pre tabIndex={0} aria-label="只读差异内容">
            {diff.content}
          </pre>
        </>
      ) : diff?.kind === "binary" ? (
        <p>这是二进制文件，无法显示文本差异。</p>
      ) : diff?.kind === "unsupported" ? (
        <p>{describeUnsupported(diff.reason)}</p>
      ) : (
        <p>
          {selected
            ? "此文件差异未能读取，请查看错误提示并刷新重试。"
            : "选择一个文件查看差异。"}
        </p>
      )}
    </div>
  );
}
