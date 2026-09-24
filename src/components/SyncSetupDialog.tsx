import type { Workbench } from "../hooks/useWorkbench";
import { useNativeDialog } from "../hooks/useNativeDialog";
import { describeGitError } from "../ui/gitPresentation";

/** 首次同步明确当前分支、远端和目标；新建远端分支需要显式勾选。 */
export function SyncSetupDialog({ workbench: w }: { workbench: Workbench }) {
  const sync = w.syncRemote;
  const dialog = useNativeDialog(sync.setup !== null, sync.closeSetup);
  return (
    <dialog
      ref={dialog.dialogRef}
      onCancel={dialog.onCancel}
      className="gm-dialog"
      aria-labelledby="sync-setup-title"
    >
      <div className="dialog-body">
        <h2 id="sync-setup-title">设置同步目标</h2>
        <p>
          当前分支：<strong>{sync.setup?.target.branchName}</strong>
        </p>
        {sync.setupWarning && <p role="status">{sync.setupWarning}</p>}
        {sync.setup?.remotes.remotes.length === 0 && (
          <p role="status">
            项目尚未配置远端。请先为项目配置远端，再重试同步。
          </p>
        )}
        <label>
          远端
          <select
            value={sync.remoteId}
            onChange={sync.selectRemote}
            disabled={sync.busy}
          >
            <option value="">请选择远端</option>
            {sync.setup?.remotes.remotes.map((remote) => (
              <option key={remote.remoteId} value={remote.remoteId}>
                {remote.name} · {remote.fetchDisplayUrl}
              </option>
            ))}
          </select>
        </label>
        <label>
          远端目标分支
          <input
            value={sync.branchName}
            onChange={sync.editBranch}
            list="sync-target-branches"
            disabled={sync.busy}
            spellCheck={false}
            autoComplete="off"
            placeholder="例如 main 或 release/stable"
          />
          <datalist id="sync-target-branches">
            {sync.suggestions.map((name) => (
              <option key={name} value={name} />
            ))}
          </datalist>
        </label>
        <label className="sync-create-option">
          <input
            type="checkbox"
            checked={sync.allowCreate}
            onChange={sync.changeCreate}
            disabled={sync.busy}
          />
          目标不存在时，允许将当前提交发布为新的远端分支
        </label>
        <p className="muted small">
          仅对当前分支生效。设置后自动同步；如双方已经分叉，将停止并提示通过合并流程处理。
        </p>
        {sync.notice && <p role="status">{sync.notice}</p>}
        {sync.error && <p role="alert">{describeGitError(sync.error)}</p>}
        {sync.busy && <p role="status">{sync.phaseLabel}</p>}
      </div>
      <div className="dialog-actions">
        <button
          className="secondary"
          disabled={sync.busy}
          onClick={sync.closeSetup}
        >
          取消
        </button>
        <button
          className="primary"
          disabled={sync.busy || !sync.remoteId || !sync.branchName.trim()}
          onClick={sync.confirm}
        >
          设置为当前分支上游并同步
        </button>
      </div>
    </dialog>
  );
}
