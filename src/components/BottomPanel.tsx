import type { Workbench } from "../hooks/useWorkbench";
import { useBottomPanel } from "../hooks/useBottomPanel";
import { OperationHistoryPanel } from "./OperationHistoryPanel";
import { Icon } from "./Icon";

/** 底部工作区的标签与面板独立于任务生命周期。 */
export function BottomPanel({ workbench: w }: { workbench: Workbench }) {
  const panel = useBottomPanel(w.showOperations, w.toggleOperations);
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
              <button disabled title="终端暂不可用">
                终端
              </button>
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
            <OperationHistoryPanel workbench={w} />
          </div>
        ))}
        {!panel.tabs.length && (
          <p className="muted bottom-empty">点击“+”打开新的标签。</p>
        )}
      </div>
    </section>
  );
}
