import type { Workbench } from "../hooks/useWorkbench";
import { useNativeDialog } from "../hooks/useNativeDialog";
import { describeGitError } from "../ui/gitPresentation";

/** 来源列表与覆盖确认分开：这里仅选择文件，下一步查看实际覆盖内容。 */
export function RestoreFilesDialog({ w }: { w: Workbench }) {
  const recovery = w.restoreFiles;
  const dialog = useNativeDialog(recovery.open, recovery.close);
  return (
    <dialog
      ref={dialog.dialogRef}
      onCancel={dialog.onCancel}
      className="gm-dialog"
      aria-labelledby="restore-files-title"
    >
      <div className="dialog-body">
        <h2 id="restore-files-title">恢复文件</h2>
        <p>选择恢复来源和文件，下一步核对覆盖内容。暂存区保持不变。</p>
        <label>
          恢复来源
          <select value={recovery.source.kind} onChange={recovery.changeSource}>
            <option value="head">当前提交（HEAD）</option>
            <option value="index">暂存区</option>
            <option value="commit">指定历史提交</option>
          </select>
        </label>
        {recovery.source.kind === "commit" && (
          <label>
            完整提交 ID
            <input
              value={recovery.source.oid}
              onChange={recovery.changeOid}
              spellCheck={false}
              placeholder="40 或 64 位完整提交 ID"
            />
          </label>
        )}
        <button
          type="button"
          className="secondary"
          onClick={recovery.reload}
          disabled={recovery.busy}
        >
          读取来源
        </button>
        {recovery.busy && <p role="status">正在读取恢复来源…</p>}
        {recovery.error && (
          <p role="alert">{describeGitError(recovery.error)}</p>
        )}
        {recovery.page && (
          <>
            <p className="small muted">
              {recovery.page.sourceOid
                ? `来源提交：${recovery.page.sourceOid}`
                : "来源：当前暂存内容"}{" "}
              · 已选择 {recovery.selected.length} 个文件
            </p>
            <div className="restore-candidates">
              {recovery.page.files.map((file) => (
                <label key={file.fileId}>
                  <input
                    type="checkbox"
                    checked={recovery.selected.includes(file.fileId)}
                    onChange={recovery.toggle(file.fileId)}
                    disabled={recovery.busy}
                  />
                  {file.path}
                </label>
              ))}
              {recovery.page.files.length === 0 && (
                <p>来源中没有可恢复的普通文件。</p>
              )}
            </div>
            {recovery.page.nextCursor && (
              <button
                className="secondary"
                disabled={recovery.busy}
                onClick={recovery.next}
              >
                下一页（保留已选文件）
              </button>
            )}
          </>
        )}
      </div>
      <div className="dialog-actions">
        <button className="secondary" onClick={recovery.close}>
          取消
        </button>
        <button
          className="primary"
          disabled={
            recovery.busy || !recovery.selected.length || !w.canOpenWrite
          }
          onClick={recovery.prepare}
        >
          预览覆盖内容
        </button>
      </div>
    </dialog>
  );
}
