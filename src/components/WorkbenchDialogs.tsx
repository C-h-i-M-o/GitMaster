import { SettingsPanel } from "./SettingsPanel";
import { SyncSetupDialog } from "./SyncSetupDialog";
import type { Workbench } from "../hooks/useWorkbench";
import { useNativeDialog } from "../hooks/useNativeDialog";
import { Icon } from "./Icon";
import { describeGitError } from "../ui/gitPresentation";
/** 克隆表单必须先取得原生目录选择器签发的父目录。 */
function CloneForm({ w }: { w: Workbench }) {
  return (
    <>
      <p className="muted">下载一个仓库到新的本地目录。</p>
      <label>
        仓库地址
        <input
          value={w.fields.cloneUrl}
          onChange={w.field("cloneUrl")}
          placeholder="https://… 或 git@…"
          autoComplete="off"
          spellCheck={false}
        />
      </label>
      <label>
        父目录
        <div className="input-action">
          <output>{w.cloneParent?.displayPath ?? "尚未选择"}</output>
          <button
            className="secondary"
            disabled={w.parentChoosing || w.preview}
            onClick={w.chooseParent}
          >
            选择目录
          </button>
        </div>
      </label>
      <label>
        新目录名称
        <input
          value={w.fields.directoryName}
          onChange={w.field("directoryName")}
          placeholder="项目目录名"
        />
      </label>
      <p className="small muted">
        目标必须不存在。系统凭据和 SSH 认证可能在下载时请求交互。
      </p>
      <button
        className="primary"
        disabled={
          w.preview ||
          w.operations.busy ||
          !w.cloneParent ||
          !w.fields.cloneUrl.trim() ||
          !w.fields.directoryName.trim()
        }
        onClick={w.prepareClone}
      >
        预览克隆
      </button>
    </>
  );
}
/** 远端操作拆为 fetch、推送和整合，所有关系评估均标记来源。 */
function RemoteForm({ w }: { w: Workbench }) {
  const remote = w.remote;
  return (
    <>
      <p className="muted">
        当前关系来自本地远端跟踪引用。最后 fetch：
        {formatFetchedAt(remote.remote?.lastFetchedAt)}
      </p>
      <label>
        远端
        <select value={w.remoteId} onChange={w.changeRemote}>
          <option value="">请选择远端</option>
          {remote.remote?.remotes.map((value) => (
            <option key={value.remoteId} value={value.remoteId}>
              {value.name} · {value.fetchDisplayUrl}
            </option>
          ))}
        </select>
      </label>
      <label>
        远端跟踪分支
        <select
          value={remote.selectedBranchId ?? ""}
          onChange={w.changeRemoteBranch}
        >
          <option value="">请选择跟踪分支</option>
          {remote.remote?.remoteBranches.map((branch) => (
            <option key={branch.remoteBranchId} value={branch.remoteBranchId}>
              {branch.name}
            </option>
          ))}
        </select>
      </label>
      {remote.assessing && <p role="status">正在比较本地对象…</p>}
      {remote.assessment && (
        <div className="assessment">
          <strong>
            {
              {
                equal: "本地与跟踪分支一致",
                ahead: "本地领先",
                behind: "本地落后",
                diverged: "双方已分叉",
                unrelated: "没有共同祖先",
                unknown: "关系尚不确定",
              }[remote.assessment.relation]
            }
          </strong>
          <span>
            领先 {remote.assessment.ahead} · 落后 {remote.assessment.behind}
          </span>
          <small>观测时间：{remote.assessment.observedAt}</small>
        </div>
      )}
      {remote.error && <p role="alert">{describeGitError(remote.error)}</p>}
      <div className="button-row">
        <button
          className="secondary"
          disabled={
            !w.canWrite("fetch") || !w.remoteId || !remote.selectedBranchId
          }
          onClick={w.prepareFetch}
        >
          预览获取更新
        </button>
        <button
          className="secondary"
          disabled={
            !w.canWrite("integrate") || remote.assessment?.relation !== "behind"
          }
          onClick={w.prepareIntegration("fastForward")}
        >
          预览快进
        </button>
        <button
          className="secondary"
          disabled={
            !w.canWrite("integrate") ||
            remote.assessment?.relation !== "diverged"
          }
          onClick={w.prepareIntegration("merge")}
        >
          预览合并
        </button>
      </div>
      <p className="small muted">
        获取只更新跟踪引用。普通合并停在提交之前，由你检查并完成提交。
      </p>
      <hr />
      <label>
        推送目标分支
        <input
          value={w.fields.targetBranch}
          onChange={w.field("targetBranch")}
          placeholder="例如 main"
          spellCheck={false}
        />
      </label>
      <p className="small muted">
        下一步会访问服务器并可能触发认证，核对目标与待推送提交后再确认。仅支持普通推送。
      </p>
      <button
        className="primary"
        disabled={
          !w.canWrite("push") || !w.remoteId || !w.fields.targetBranch.trim()
        }
        onClick={w.preparePush}
      >
        查询服务器并预览推送
      </button>
    </>
  );
}
/** 原生模态承载业务表单，执行仍统一走单独的写入确认框。 */
export function WorkbenchDialogs({ workbench: w }: { workbench: Workbench }) {
  const dialog = useNativeDialog(w.modal !== null, w.closeModal);
  const refreshRemote = useNativeDialog(
    w.manualRefresh.choices !== null,
    w.manualRefresh.cancel,
  );
  const discard = useNativeDialog(w.discardPending, w.keepDraft);
  const settingsDiscard = useNativeDialog(
    w.settingsPending !== null,
    w.keepSettings,
  );
  return (
    <>
      <SyncSetupDialog workbench={w} />
      <dialog
        ref={refreshRemote.dialogRef}
        onCancel={refreshRemote.onCancel}
        className="gm-dialog"
        aria-labelledby="refresh-remote-title"
      >
        <div className="dialog-body">
          <h2 id="refresh-remote-title">选择要获取更新的远端</h2>
          <p className="muted">
            将获取所选远端的全部分支更新，本地分支和文件保持不变。
          </p>
          <label>
            远端
            <select
              value={w.manualRefresh.selected}
              onChange={w.manualRefresh.select}
            >
              <option value="">请选择远端</option>
              {w.manualRefresh.choices?.remotes.map((remote) => (
                <option key={remote.remoteId} value={remote.remoteId}>
                  {remote.name} · {remote.fetchDisplayUrl}
                </option>
              ))}
            </select>
          </label>
        </div>
        <div className="dialog-actions">
          <button className="secondary" onClick={w.manualRefresh.cancel}>
            取消
          </button>
          <button
            className="primary"
            disabled={
              !w.manualRefresh.selected ||
              w.manualRefresh.busy ||
              w.operations.busy
            }
            onClick={w.manualRefresh.confirm}
          >
            获取更新并刷新
          </button>
        </div>
      </dialog>
      <dialog
        ref={dialog.dialogRef}
        onCancel={dialog.onCancel}
        className={`gm-dialog ${w.modal === "settings" ? "settings-dialog" : ""}`}
        aria-labelledby="form-title"
      >
        <header className="drawer-header">
          <h2 id="form-title">
            {w.modal === "clone"
              ? "克隆仓库"
              : w.modal === "branch"
                ? "创建分支"
                : w.modal === "remote"
                  ? "远端协作"
                  : "应用设置"}
          </h2>
          <button
            className="icon-button"
            aria-label="关闭弹窗"
            onClick={w.closeModal}
          >
            <Icon name="close" />
          </button>
        </header>
        <div className="dialog-body">
          {w.modal === "clone" ? (
            <CloneForm w={w} />
          ) : w.modal === "remote" ? (
            <RemoteForm w={w} />
          ) : w.modal === "branch" ? (
            <>
              <p className="muted">
                从当前提交创建新分支，创建后仍停留在当前分支。
              </p>
              <label>
                分支名称
                <input
                  value={w.fields.branchName}
                  onChange={w.field("branchName")}
                  placeholder="feature/…"
                  spellCheck={false}
                />
              </label>
              <button
                className="primary"
                disabled={
                  !w.canWrite("createBranch") || !w.fields.branchName.trim()
                }
                onClick={w.prepareBranch}
              >
                创建分支
              </button>
            </>
          ) : w.modal === "settings" ? (
            <SettingsPanel w={w} />
          ) : null}
          {w.operations.error && (
            <p role="alert">{describeGitError(w.operations.error)}</p>
          )}
          {w.resourceError && (
            <p role="alert">{describeGitError(w.resourceError)}</p>
          )}
          {w.operations.activity === "preparing" && (
            <p role="status">正在准备预览，请稍候…</p>
          )}
        </div>
      </dialog>
      <dialog
        ref={settingsDiscard.dialogRef}
        onCancel={settingsDiscard.onCancel}
        className="gm-dialog"
        aria-labelledby="settings-discard-title"
      >
        <div className="dialog-body">
          <h2 id="settings-discard-title">设置尚未应用</h2>
          <p>是否保存当前设置更改？</p>
          {w.appSettings.error && (
            <p role="alert">保存失败，草稿已保留。请继续编辑并检查提示。</p>
          )}
        </div>
        <div className="dialog-actions">
          <button
            className="secondary"
            disabled={w.appSettings.activity !== "idle"}
            onClick={w.keepSettings}
          >
            继续编辑
          </button>
          <button
            className="secondary"
            disabled={w.appSettings.activity !== "idle"}
            onClick={w.discardSettings}
          >
            放弃更改
          </button>
          <button
            className="primary"
            disabled={w.appSettings.activity !== "idle" || w.operations.busy}
            onClick={w.saveSettingsAndContinue}
          >
            保存并继续
          </button>
        </div>
      </dialog>
      <dialog
        ref={discard.dialogRef}
        onCancel={discard.onCancel}
        className="gm-dialog"
        aria-labelledby="discard-title"
      >
        <div className="dialog-body">
          <h2 id="discard-title">放弃未保存的冲突草稿？</h2>
          <p>草稿仍在此窗口中。放弃后将重新读取磁盘内容。</p>
        </div>
        <div className="dialog-actions">
          <button className="secondary" onClick={w.keepDraft}>
            继续编辑
          </button>
          <button className="primary" onClick={w.discardDraft}>
            放弃草稿并继续
          </button>
        </div>
      </dialog>
    </>
  );
}
import { formatFetchedAt } from "../ui/refreshResult";
