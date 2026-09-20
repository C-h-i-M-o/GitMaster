import { MotionConfig, motion } from "motion/react";
import { useAppInfo } from "./hooks/useAppInfo";
import { describeRuntime, entranceTransition } from "./ui/presentation";

/** 展示工程起始页与真实运行模式，不模拟尚未实现的 Git 功能。 */
export default function App() {
  const state = useAppInfo();

  return (
    <MotionConfig reducedMotion="user">
      <div className="app-shell">
        <aside className="sidebar" aria-label="应用介绍">
          <a className="brand" href="#welcome" aria-label="gitMaster 起始页">
            <span className="brand-symbol" aria-hidden="true">
              g
            </span>
            <span>
              git<span className="brand-weight">Master</span>
            </span>
          </a>
          <div className="sidebar-body">
            <span className="section-caption">你的版本空间</span>
            <a className="navigation-item" href="#welcome" aria-current="page">
              <span aria-hidden="true">↗</span> 从这里开始
            </a>
            <p className="sidebar-note">
              让修改有迹可循，
              <br />
              让尝试更有底气。
            </p>
          </div>
          <div className="sidebar-footer">
            <span className="status-dot" aria-hidden="true" />
            工程起点 · M0
          </div>
        </aside>

        <main id="welcome" className="workspace">
          <header className="workspace-header">
            <span>开始 / 欢迎</span>
            <span className="phase-badge">开发预览</span>
          </header>

          <motion.section
            className="welcome-content"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            transition={entranceTransition}
            aria-labelledby="welcome-title"
          >
            <p className="eyebrow">A LITTLE CLARITY. EVERY VERSION.</p>
            <h1 id="welcome-title">
              每一次修改，
              <br />
              <span>都值得被记住。</span>
            </h1>
            <p className="intro">
              用清晰的方式管理版本，放心迈出下一步。
              <br />
              gitMaster 的跨平台工程已就位，Git 功能将逐步加入。
            </p>

            <section
              className="foundation-panel"
              aria-labelledby="foundation-title"
            >
              <div className="panel-heading">
                <span className="small-symbol" aria-hidden="true">
                  01
                </span>
                <div>
                  <h2 id="foundation-title">先准备好你的 Git</h2>
                  <p>应用使用你电脑上的 Git，不附带或自动安装。</p>
                </div>
              </div>
              <div className="installation-address">
                <span>官方安装地址</span>
                <code>https://git-scm.com/install/</code>
              </div>
              <p className="panel-footnote">
                当前尚未检测 Git。安装引导、路径设置和仓库操作将在后续阶段提供。
              </p>
            </section>

            <div className="architecture-note">
              <div>
                <span className="note-number">01 / 界面</span>
                <p>React + TypeScript</p>
              </div>
              <div>
                <span className="note-number">02 / 桌面</span>
                <p>Tauri 2</p>
              </div>
              <div>
                <span className="note-number">03 / 核心</span>
                <p>独立 Rust 核心</p>
              </div>
            </div>
          </motion.section>

          <footer className="workspace-footer">
            <p role="status" aria-live="polite">
              {describeRuntime(state)}
            </p>
            <span>Windows / macOS</span>
          </footer>
        </main>
      </div>
    </MotionConfig>
  );
}
