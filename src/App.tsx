import { useWorkbench } from "./hooks/useWorkbench";
import { WorkspaceToolbar } from "./components/WorkspaceToolbar";
import { Icon } from "./components/Icon";
import { CommitGraph } from "./components/CommitGraph";
import { WorkbenchDrawer } from "./components/WorkbenchDrawer";
import { BottomPanel } from "./components/BottomPanel";
import { ChangeDetailDrawer } from "./components/ChangeDetailDrawer";
import { WorkbenchDialogs } from "./components/WorkbenchDialogs";
import { WriteConfirmation } from "./components/WriteConfirmation";
import { describeGitError } from "./ui/gitPresentation";
/** 按 HTML 设计稿组合真实工作台，事件与副作用交给独立 hook。 */
export default function App() {
  const w = useWorkbench();
  const repository = w.repo.repository;
  return (
    <div className="app-shell">
      <WorkspaceToolbar workbench={w} />
      <div className="workspace" id="workspace">
        <aside className="sidebar" aria-label="项目导航">
          <div className="project-heading">
            <span className="eyebrow">WORKSPACE</span>
            <strong title={repository?.rootPath}>
              {repository?.rootPath.split(/[\\/]/).filter(Boolean).at(-1) ??
                "尚未打开项目"}
            </strong>
            <small title={repository?.rootPath}>
              {repository?.rootPath ?? "让每个版本更清楚"}
            </small>
          </div>
          <nav className="workspace-nav">
            <button
              className={
                w.drawer === null || w.drawer === "detail" ? "active" : ""
              }
              onClick={w.openDrawer(null)}
            >
              <Icon name="history" />
              <span>提交历史</span>
            </button>
            <button
              className={w.drawer === "changes" ? "active" : ""}
              disabled={!repository}
              onClick={w.openDrawer("changes")}
            >
              <Icon name="changes" />
              <span>本地修改</span>
              <b>{repository?.changes.length ?? 0}</b>
            </button>
            <button
              className={w.drawer === "files" ? "active" : ""}
              disabled={!repository}
              onClick={w.openDrawer("files")}
            >
              <Icon name="files" />
              <span>项目文件</span>
            </button>
            {repository?.operations.includes("merge") && (
              <button
                className={w.drawer === "conflicts" ? "active" : ""}
                onClick={w.openDrawer("conflicts")}
              >
                <Icon name="merge" />
                <span>合并与冲突</span>
              </button>
            )}
          </nav>
          <section className="branches">
            <div className="section-heading">
              <button
                className="eyebrow branch-collapse"
                aria-expanded={w.branchesExpanded}
                onClick={w.toggleBranches}
              >
                <Icon name="chevron" />
                分支
              </button>
              <button
                className="icon-button"
                title="新建分支"
                aria-label="新建分支"
                disabled={!w.canOpenWrite}
                onClick={w.openModal("branch")}
              >
                <Icon name="plus" />
              </button>
            </div>
            {w.branchesExpanded &&
              w.branches?.branches.map((branch) => (
                <button
                  key={branch.branchId}
                  className={`branch-item ${branch.current ? "current" : ""}`}
                  onClick={w.switchBranch(branch.branchId)}
                  disabled={
                    branch.current ||
                    branch.kind === "remote" ||
                    branch.occupiedByOtherWorktree ||
                    !w.canSwitchBranch
                  }
                  title={`${branch.name}${branch.occupiedByOtherWorktree ? " · 其他工作树正在使用" : ""}`}
                >
                  <span className={`branch-dot ${branch.kind}`} />
                  <span>{branch.name}</span>
                  {branch.current && <Icon name="check" />}
                  {branch.occupiedByOtherWorktree && <small>占用</small>}
                </button>
              ))}
            {repository?.head.kind === "unborn" && (
              <p className="muted small">
                {repository.head.name} · 等待首次提交
              </p>
            )}
            {repository?.head.kind === "detached" && (
              <p className="warning-card small">
                分离头指针 · {repository.head.oid.slice(0, 7)}
              </p>
            )}
          </section>
          <div className="sidebar-bottom">
            <span
              className={`status-dot ${w.git.environment?.status === "ready" ? "ready" : ""}`}
            />
            <span>
              {w.preview
                ? "浏览器界面预览"
                : w.git.environment?.status === "ready"
                  ? `Git ${w.git.environment.version}`
                  : "Git 尚未就绪"}
            </span>
            {!w.preview && w.git.environment?.status !== "ready" && (
              <button onClick={w.openModal("settings")}>配置 Git</button>
            )}
          </div>
        </aside>
        <main className="main-content">
          <div className="workspace-alerts" aria-live="polite">
            {w.syncRemote.notice && (
              <p role="status">
                {w.syncRemote.notice}{" "}
                {w.syncRemote.diverged && (
                  <button onClick={w.openModal("remote")}>打开合并流程</button>
                )}
              </p>
            )}
            {w.manualRefresh.notice && (
              <p role="status">{w.manualRefresh.notice}</p>
            )}
            {w.manualRefresh.resultLabel && (
              <p role="status">{w.manualRefresh.resultLabel}</p>
            )}
            {[
              w.actionError,
              w.resourceError,
              w.repo.error,
              w.operations.error,
              w.manualRefresh.error,
              w.syncRemote.error,
            ]
              .filter((value) => value !== null)
              .map((error, index) => (
                <p role="alert" key={`${error.code}-${index}`}>
                  {describeGitError(error)}
                </p>
              ))}
            {w.repo.stale && (
              <p role="alert">
                仓库刷新失败，正在展示旧状态。请刷新后继续写操作。
              </p>
            )}
            {w.operations.activity === "preparing" && (
              <p role="status">正在准备操作预览…</p>
            )}
            {w.operations.activity === "unverified" && (
              <p role="alert">
                任务状态尚未核实。
                <button onClick={w.operations.resume}>重新查询任务</button>
              </p>
            )}
          </div>
          <div className="canvas-area">
            <CommitGraph workbench={w} />
            <WorkbenchDrawer workbench={w} />
            <ChangeDetailDrawer workbench={w} />
          </div>
          <BottomPanel workbench={w} />
        </main>
      </div>
      <footer className="statusbar">
        <span>
          {repository
            ? repository.head.kind === "detached"
              ? "分离头指针"
              : repository.head.name
            : "就绪后打开一个仓库"}
        </span>
        <span>
          {w.preview
            ? "界面预览，不连接 Git"
            : `${w.history.page?.commits.length ?? 0} 个提交 · ${w.branches?.branches.length ?? 0} 个分支 · 操作后核实`}
        </span>
      </footer>
      <WorkbenchDialogs workbench={w} />
      <WriteConfirmation workbench={w} />
    </div>
  );
}
