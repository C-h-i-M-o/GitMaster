import type { Workbench } from "../hooks/useWorkbench";
import { useBottomPanel } from "../hooks/useBottomPanel";
import { OperationHistoryPanel } from "./OperationHistoryPanel";
import { Icon } from "./Icon";
import { TerminalPanel } from "../ui/terminalLoader";
import { Suspense } from "react";
import { useNativeDialog } from "../hooks/useNativeDialog";
import { defaultSettings } from "../ui/settingsDraft";

/** 底部工作区的标签与面板独立于任务生命周期。 */
export function BottomPanel({ workbench: w }: { workbench: Workbench }) {
  const preferences = w.appSettings.saved?.settings.terminal ?? null;
  const panel = useBottomPanel(
    w.showOperations,
    w.toggleOperations,
    w.repo.repository?.repositoryId ?? null,
    preferences,
    w.guardExit,
    w.exitProtected,
  );
  const closing = useNativeDialog(
    panel.pendingClose !== null || panel.quitPending,
    panel.cancelClose,
  );
  return (
    <section
      ref={panel.panel}
      className={`bottom-panel ${w.showOperations ? "expanded" : ""}`}
      aria-label="底部工作区"
      style={{ height: w.showOperations ? panel.height : 40 }}
    >
      {w.showOperations && (
        <div
          className="bottom-resize"
          role="separator"
          aria-label="调整底部面板高度"
          aria-orientation="horizontal"
          aria-valuenow={panel.height}
          aria-valuemin={120}
          aria-valuemax={panel.maximumHeight}
          aria-valuetext={`${panel.height} 像素`}
          tabIndex={0}
          onPointerDown={panel.startResize}
          onPointerMove={panel.resize}
          onPointerUp={panel.endResize}
          onPointerCancel={panel.endResize}
          onKeyDown={panel.resizeKey}
        />
      )}
      <header className="bottom-toolbar">
        <div
          className="bottom-tabs"
          role="tablist"
          aria-label="底部标签"
          onKeyDown={panel.tabKey}
        >
          {panel.tabs.map((tab) => (
            <div
              className={`bottom-tab ${panel.activeId === tab.id ? "active" : ""}`}
              key={tab.id}
            >
              <button
                role="tab"
                id={`tab-${tab.id}`}
                data-bottom-tab={tab.id}
                aria-selected={panel.activeId === tab.id}
                aria-controls={`pane-${tab.id}`}
                tabIndex={panel.activeId === tab.id ? 0 : -1}
                onClick={panel.select(tab.id)}
              >
                <Icon name="activity" />
                {tab.title}
              </button>
              <button
                className="tab-close"
                onClick={panel.close(tab.id)}
                aria-label={`关闭${tab.title}`}
              >
                <Icon name="close" />
              </button>
            </div>
          ))}
        </div>
        <div className="bottom-add">
          <button
            className="icon-button"
            onClick={panel.toggleMenu}
            aria-expanded={panel.menu}
            aria-label="新建底部标签"
          >
            <Icon name="plus" />
          </button>
          {panel.menu && (
            <div className="bottom-add-menu">
              <button onClick={panel.addOperations}>
                <Icon name="activity" />
                操作记录
              </button>
              {preferences?.profiles.map((profile) => (
                <button
                  key={profile.profileId}
                  onClick={panel.addTerminal(profile.profileId)}
                  disabled={w.preview || panel.terminalBusy}
                >
                  终端 · {profile.name}
                  {profile.profileId === preferences.defaultProfileId
                    ? "（默认）"
                    : ""}
                </button>
              ))}
              {!preferences && (
                <button disabled>终端（请先加载桌面设置）</button>
              )}
            </div>
          )}
        </div>
        <button
          className="icon-button"
          onClick={w.toggleOperations}
          aria-expanded={w.showOperations}
          aria-label={w.showOperations ? "折叠底部面板" : "展开底部面板"}
        >
          <Icon name="chevron" />
        </button>
      </header>
      {panel.terminalMessage && <p role="alert">{panel.terminalMessage}</p>}
      <div className="bottom-content" hidden={!w.showOperations}>
        {panel.tabs.map((tab) => (
          <div
            className="bottom-pane"
            id={`pane-${tab.id}`}
            role="tabpanel"
            aria-labelledby={`tab-${tab.id}`}
            hidden={panel.activeId !== tab.id}
            key={tab.id}
          >
            {tab.kind === "operations" ? (
              <OperationHistoryPanel workbench={w} />
            ) : (
              <Suspense fallback={<p role="status">正在加载终端视图…</p>}>
                <TerminalPanel
                  session={tab.session}
                  preferences={preferences ?? defaultSettings().terminal}
                />
              </Suspense>
            )}
          </div>
        ))}
        {!panel.tabs.length && (
          <p className="muted bottom-empty">点击“+”打开新的标签。</p>
        )}
      </div>
      <dialog
        ref={closing.dialogRef}
        onCancel={closing.onCancel}
        className="gm-dialog"
        aria-label="确认结束终端"
      >
        <div className="dialog-body">
          <h2>{panel.quitPending ? "结束终端并退出？" : "关闭终端？"}</h2>
          <p>此操作会结束对应 Shell 及终端中的任务。未完成的命令可能被中断。</p>
          {panel.terminalMessage && <p role="alert">{panel.terminalMessage}</p>}
        </div>
        <div className="dialog-actions">
          <button disabled={panel.terminalBusy} onClick={panel.cancelClose}>
            取消
          </button>
          <button disabled={panel.terminalBusy} onClick={panel.confirmClose}>
            {panel.terminalBusy ? "正在结束…" : "结束终端"}
          </button>
        </div>
      </dialog>
    </section>
  );
}
