import type { FileDiff } from "../types/git";
import { describeUnsupported } from "../ui/gitPresentation";
/** 共用只读文本展示，二进制、受限内容与截断均明确标注。 */
export function ContentPreview({
  value,
  loading,
}: {
  value: FileDiff | null;
  loading: boolean;
}) {
  return (
    <div className="content-preview">
      {loading ? (
        <p role="status">正在读取内容…</p>
      ) : value?.kind === "text" ? (
        <>
          {value.truncated && <p role="status">内容已截断，仅展示受限预览。</p>}
          <pre tabIndex={0} aria-label="只读文件内容">
            {value.content || "（没有文本差异）"}
          </pre>
        </>
      ) : value?.kind === "binary" ? (
        <p>二进制文件，无法显示文本内容。</p>
      ) : value?.kind === "unsupported" ? (
        <p>暂不支持预览：{describeUnsupported(value.reason)}</p>
      ) : (
        <p>选择文件查看内容。</p>
      )}
    </div>
  );
}
