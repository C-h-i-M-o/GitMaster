import type { Workbench } from "../hooks/useWorkbench";
import { Icon } from "./Icon";
import { shortcutLabel } from "../ui/shortcuts";
/** 项目菜单呈现持久化的最近记录，工具栏为已有业务入口。 */
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
            <button
              disabled={!w.canOpen}
              onClick={w.open}
              title={`打开本地项目（${shortcutLabel("O")}）`}
            >
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
            {w.recentProjects.length > 0 && <p className="muted small">最近</p>}
            {w.recentProjectsError && (
              <p className="recent-project-error" role="alert">
                {w.recentProjectsError}{" "}
                <button onClick={w.refreshRecentProjects}>重试</button>
              </p>
            )}
            {w.recentProjects.map((project) => (
              <button
                className="recent-project"
                key={project.rootPath}
                disabled={!w.canOpen}
                onClick={w.openRecent(project.rootPath)}
                title={project.rootPath}
              >
                <span className="recent-project-avatar" aria-hidden="true">
                  {project.name.slice(0, 1).toUpperCase()}
                </span>
                <span className="recent-project-info">
                  <strong>{project.name}</strong>
                  <small>{project.rootPath}</small>
                </span>
                {project.rootPath === repository?.rootPath && (
                  <span aria-label="当前项目">✓</span>
                )}
              </button>
            ))}
          </div>
        )}
      </div>
      <div className="toolbar" aria-label="仓库操作">
        <button
          className="local-refresh"
          data-tooltip="刷新本地状态"
          aria-label="刷新本地状态"
          onClick={w.refreshLocal}
          disabled={
            !repository ||
            w.preview ||
            w.repo.loading ||
            w.operations.busy ||
            Boolean(w.editor.savingPath) ||
            w.manualRefresh.busy ||
            w.syncRemote.busy ||
            w.conflicts.dirty
          }
        >
          <Icon name="refresh" />
        </button>
        <div
          className="sync-action"
          data-tooltip={w.syncReason() || "同步远程"}
        >
          <button
            aria-label="同步远程"
            onClick={w.syncRemote.run}
            disabled={Boolean(w.syncReason())}
          >
            <Icon name="sync" />
          </button>
        </div>
        <div className="sync-action" data-tooltip="获取远端更新">
          <button
            aria-label="获取远端更新"
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
            <Icon name="fetch" />
          </button>
        </div>
      </div>
      <button
        className="icon-button settings-button"
        title={`应用设置（${shortcutLabel(",")}）`}
        aria-label="应用设置"
        onClick={w.openModal("settings")}
      >
        <Icon name="settings" />
      </button>
    </header>
  );
}
