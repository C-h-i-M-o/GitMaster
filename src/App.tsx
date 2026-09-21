import { useWorkspace } from "./hooks/useWorkspace";
import { useAppInfo } from "./hooks/useAppInfo";
import { describeRuntime } from "./ui/presentation";
import { describeGitError } from "./ui/gitPresentation";
import { GitEnvironmentPanel } from "./components/GitEnvironmentPanel";
import { RepositoryView } from "./components/RepositoryView";
/** 组合只读工作区；交互方法及副作用由独立 hook 提供。 */
export default function App() {
  const workspace = useWorkspace();
  const app = useAppInfo();
  return (
    <div className="app-shell">
      <aside className="sidebar" aria-label="应用导航">
        <a className="brand" href="#workspace">
          <span className="brand-symbol" aria-hidden="true">
            g
          </span>
          <span>
            git<span className="brand-weight">Master</span>
          </span>
        </a>
        <div className="sidebar-body">
          <span className="section-caption">你的版本空间</span>
          <a className="navigation-item" href="#workspace" aria-current="page">
            ↗ 工作区
          </a>
          <p className="sidebar-note">
            让修改有迹可循，
            <br />
            让尝试更有底气。
          </p>
        </div>
        <div className="sidebar-footer">
          <span className="status-dot" aria-hidden="true" />
          M1 · 只读工作区
        </div>
      </aside>
      <main id="workspace" className="workspace">
        <header className="workspace-header">
          <span>工作区 / 环境与仓库</span>
          <span className="phase-badge">开发预览</span>
        </header>
        <section className="m1-content" aria-labelledby="workspace-title">
          <p className="eyebrow">A LITTLE CLARITY. EVERY VERSION.</p>
          <h1 id="workspace-title">
            看见每一次修改，
            <br />
            <span>再决定下一步。</span>
          </h1>
          <p className="intro">查看本地状态和差异，让每个版本更清楚。</p>
          <GitEnvironmentPanel
            {...workspace.git}
            onRefresh={workspace.git.refresh}
            onChoose={workspace.chooseGit}
            onReset={workspace.resetGit}
            onInstall={workspace.install}
            choosing={workspace.choosing}
          />
          {workspace.actionError && (
            <p role="alert">{describeGitError(workspace.actionError)}</p>
          )}
          {!workspace.preview && (
            <RepositoryView
              {...workspace.repo}
              onOpen={workspace.open}
              onRefresh={workspace.repo.refresh}
              onSelect={workspace.repo.selectDiff}
              disabled={!workspace.canOpen}
            />
          )}
        </section>
        <footer className="workspace-footer">
          <p role="status">{describeRuntime(app)}</p>
          <span>Windows / macOS</span>
        </footer>
      </main>
    </div>
  );
}
