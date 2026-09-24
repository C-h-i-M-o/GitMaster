import type { Workbench } from "../hooks/useWorkbench";
import { Icon } from "./Icon";
import { formatFetchedAt } from "../ui/refreshResult";
/** 项目菜单仅呈现本会话记录，工具栏为已有业务入口。 */
export function WorkspaceToolbar({ workbench: w }: { workbench: Workbench }) {
  const repository = w.repo.repository;
  return (
    <header className="topbar">
      <a className="brand" href="#workspace" aria-label="gitMaster 工作台">
        <span className="brand-mark">
          <Icon name="branch" />
        </span>
        <span>
          git<strong>Master</strong>
        </span>
      </a>
      <div className="project-switcher">
        <button
          className="project-button"
          onClick={w.toggleProjectMenu}
          aria-expanded={w.projectMenu}
        >
          <Icon name="folder" />
          <span>
            <strong>
              {repository?.rootPath.split(/[\\/]/).filter(Boolean).at(-1) ??
                "选择项目"}
            </strong>
            <small title={repository?.rootPath}>
              {repository?.rootPath ?? "打开或下载仓库"}
            </small>
          </span>
          <Icon name="chevron" />
        </button>
        {w.projectMenu && (
          <div className="project-menu">
            <button disabled={!w.canOpen} onClick={w.open}>
              <Icon name="folder" />
              打开本地项目
            </button>
            <button
              disabled={
                w.preview ||
                w.git.environment?.status !== "ready" ||
                w.operations.busy ||
                w.conflicts.dirty
              }
              onClick={w.openModal("clone")}
            >
              <Icon name="download" />
              下载项目
            </button>
            {w.recentProjects.length > 0 && (
              <p className="muted small">本次会话</p>
            )}
            {w.recentProjects.map((path) => (
              <button
                key={path}
                disabled={!w.canOpen}
                onClick={w.openRecent(path)}
                title={path}
              >
                {path}
              </button>
            ))}
          </div>
        )}
      </div>
      <div className="toolbar" aria-label="仓库操作">
        <div className="sync-action">
          <button
            title={w.syncReason() || "同步当前分支的远端更新"}
            aria-label="同步远程"
            aria-describedby="sync-reason"
            onClick={w.syncRemote.run}
            disabled={Boolean(w.syncReason())}
          >
            <Icon name="upload" />
            <span>{w.syncRemote.phaseLabel}</span>
          </button>
          <small id="sync-reason" className="sync-hint">
            {w.syncReason()}
          </small>
        </div>
        <div className="sync-action">
          <button
            title="获取远端全部分支的更新并刷新，不合并本地分支"
            aria-describedby="last-fetch-time"
            onClick={w.manualRefresh.run}
            disabled={
              !repository ||
              w.preview ||
              w.manualRefresh.busy ||
              w.syncRemote.busy ||
              w.operations.busy ||
              w.conflicts.dirty ||
              w.repo.loading
            }
          >
            <Icon name="refresh" />
            <span>{w.manualRefresh.busy ? "正在刷新…" : "刷新"}</span>
          </button>
          <small id="last-fetch-time" className="sync-hint">
            最近获取：{formatFetchedAt(w.remote.remote?.lastFetchedAt)}
          </small>
        </div>
      </div>
      <button
        className="icon-button settings-button"
        title="应用设置"
        aria-label="应用设置"
        onClick={w.openModal("settings")}
        disabled={w.operations.busy || w.syncRemote.busy || w.conflicts.dirty}
      >
        <Icon name="settings" />
      </button>
    </header>
  );
}
