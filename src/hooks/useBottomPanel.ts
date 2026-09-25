import {
  useEffect,
  useRef,
  useState,
  type KeyboardEvent,
  type PointerEvent,
} from "react";
import {
  addBottomTab,
  closeBottomTab,
  moveBottomTab,
  type BottomTabsState,
} from "../ui/bottomTabs";
import {
  createTerminal,
  closeTerminal,
  terminalError,
} from "../services/terminal";
import type { TerminalPreferences } from "../types/settings";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { isTauri } from "@tauri-apps/api/core";

/** 管理底部标签和面板尺寸；折叠或切标签不卸载记录视图。 */
export function useBottomPanel(
  expanded: boolean,
  toggleExpanded: () => void,
  repositoryId: string | null,
  preferences: TerminalPreferences | null,
  guardExit: (action: () => Promise<void>) => void,
  exitProtected: boolean,
) {
  const draftExit = useRef({ guardExit, exitProtected });
  draftExit.current = { guardExit, exitProtected };
  const [state, setState] = useState<BottomTabsState>({
    tabs: [{ id: "operations-1", kind: "operations", title: "操作记录" }],
    activeId: "operations-1",
  });
  const sequence = useRef(1);
  const current = useRef(state);
  current.current = state;
  const alive = useRef(true);
  const busy = useRef(false);
  const [terminalBusy, setTerminalBusy] = useState(false);
  const [terminalMessage, setTerminalMessage] = useState<string | null>(null);
  const [pendingClose, setPendingClose] = useState<string | null>(null);
  const [quitPending, setQuitPending] = useState(false);
  const exitAllowed = useRef(false);
  useEffect(() => {
    alive.current = true;
    let unlisten: (() => void) | undefined;
    if (isTauri())
      void getCurrentWindow()
        .onCloseRequested((event) => {
          if (
            !exitAllowed.current &&
            (draftExit.current.exitProtected ||
              busy.current ||
              current.current.tabs.some((tab) => tab.kind === "terminal"))
          ) {
            event.preventDefault();
            draftExit.current.guardExit(async () => {
              if (
                busy.current ||
                current.current.tabs.some((tab) => tab.kind === "terminal")
              )
                setQuitPending(true);
              else {
                exitAllowed.current = true;
                try {
                  await getCurrentWindow().close();
                } catch (cause: unknown) {
                  exitAllowed.current = false;
                  setTerminalMessage(terminalError(cause));
                }
              }
            });
          }
        })
        .then((stop) => {
          if (alive.current) unlisten = stop;
          else stop();
        });
    return () => {
      alive.current = false;
      unlisten?.();
      for (const tab of current.current.tabs)
        if (tab.kind === "terminal")
          void closeTerminal(tab.session.sessionId).catch(() => {});
    };
  }, []);
  const panel = useRef<HTMLElement>(null);
  const [menu, setMenu] = useState(false);
  const [height, setHeight] = useState(300);
  const [maximumHeight, setMaximumHeight] = useState(410);
  useEffect(() => {
    const parent = panel.current?.parentElement;
    if (!parent) return;
    /** 可用空间变小时收回面板，并同步辅助功能的数值范围。 */
    function measure(): void {
      const maximum = Math.max(120, (parent?.clientHeight ?? 650) - 360);
      setMaximumHeight(maximum);
      setHeight((current) => Math.min(maximum, Math.max(120, current)));
    }
    const observer = new ResizeObserver(measure);
    observer.observe(parent);
    measure();
    return () => observer.disconnect();
  }, []);
  const drag = useRef<{ pointerId: number; y: number; height: number } | null>(
    null,
  );
  /** 新建独立筛选的记录视图，共享同一操作数据源。 */
  function addOperations(): void {
    const number = ++sequence.current;
    setState((old) =>
      addBottomTab(old, {
        id: `operations-${number}`,
        kind: "operations",
        title: `操作记录 ${number}`,
      }),
    );
    setMenu(false);
    if (!expanded) toggleExpanded();
  }
  /** 仅用户点击已保存配置时创建 shell，并保留创建时项目归属。 */
  function addTerminal(profileId: string): () => void {
    return () => {
      void startTerminal(profileId);
    };
  }
  /** 创建期间禁止重复点击；视图已关闭时回收迟到会话。 */
  async function startTerminal(profileId: string): Promise<void> {
    if (busy.current || !preferences || !isTauri()) return;
    busy.current = true;
    setTerminalBusy(true);
    setTerminalMessage(null);
    try {
      const session = await createTerminal(repositoryId, profileId);
      if (!alive.current) {
        await closeTerminal(session.sessionId);
        return;
      }
      const number = ++sequence.current;
      const project =
        session.displayCwd.split(/[\\/]/).filter(Boolean).at(-1) ??
        session.displayCwd;
      const shell = session.shellLabel.split(/[\\/]/).at(-1) ?? "Shell";
      setState((old) =>
        addBottomTab(old, {
          id: `terminal-${number}`,
          kind: "terminal",
          title: `${shell} · ${project}`,
          session,
        }),
      );
      setMenu(false);
      if (!expanded) toggleExpanded();
    } catch (cause: unknown) {
      if (alive.current) setTerminalMessage(terminalError(cause));
    } finally {
      busy.current = false;
      if (alive.current) setTerminalBusy(false);
    }
  }
  /** 标签激活只改变可见项，不清除其筛选或滚动。 */
  function select(id: string): () => void {
    return () => {
      setState((old) => ({ ...old, activeId: id }));
      if (!expanded) toggleExpanded();
    };
  }
  /** 关闭标签后仍保留操作控制器中的任务。 */
  function close(id: string): () => void {
    return () => {
      if (
        current.current.tabs.find((tab) => tab.id === id)?.kind === "terminal"
      )
        setPendingClose(id);
      else setState((old) => closeBottomTab(old, id));
    };
  }
  /** 用户取消关闭时保持所有会话、输出与输入状态。 */
  function cancelClose(): void {
    if (!busy.current) {
      setPendingClose(null);
      setQuitPending(false);
    }
  }
  /** 明确确认后先回收会话，再移除标签或退出窗口，失败保留重试入口。 */
  async function confirmClose(): Promise<void> {
    if (busy.current) return;
    busy.current = true;
    setTerminalBusy(true);
    setTerminalMessage(null);
    try {
      const targets = current.current.tabs.filter(
        (tab) =>
          tab.kind === "terminal" && (quitPending || tab.id === pendingClose),
      );
      for (const tab of targets) {
        if (tab.kind !== "terminal") continue;
        await closeTerminal(tab.session.sessionId);
        setState((old) => closeBottomTab(old, tab.id));
      }
      setPendingClose(null);
      if (quitPending) {
        exitAllowed.current = true;
        try {
          await getCurrentWindow().close();
        } catch (cause: unknown) {
          exitAllowed.current = false;
          throw cause;
        }
      }
      setQuitPending(false);
    } catch (cause: unknown) {
      setTerminalMessage(terminalError(cause));
    } finally {
      busy.current = false;
      if (alive.current) setTerminalBusy(false);
    }
  }
  /** 显式打开新建菜单，不触发其他标签动作。 */
  function toggleMenu(): void {
    setMenu((value) => !value);
  }
  /** 根据可用工作台高度限制面板尺寸，保持图和操作栏可达。 */
  function bounded(value: number): number {
    return Math.min(
      Math.max(120, (panel.current?.parentElement?.clientHeight ?? 650) - 360),
      Math.max(120, value),
    );
  }
  /** 拖动分隔条时捕获指针，防止移出面板后丢失结束事件。 */
  function startResize(event: PointerEvent<HTMLDivElement>): void {
    if (!expanded || event.button !== 0) return;
    event.preventDefault();
    event.currentTarget.setPointerCapture(event.pointerId);
    drag.current = { pointerId: event.pointerId, y: event.clientY, height };
  }
  /** 调整视觉高度不销毁活跃标签内容。 */
  function resize(event: PointerEvent<HTMLDivElement>): void {
    const active = drag.current;
    if (active?.pointerId === event.pointerId)
      setHeight(bounded(active.height + active.y - event.clientY));
  }
  /** 结束拖动并释放指针捕获，系统取消使用相同路径。 */
  function endResize(event: PointerEvent<HTMLDivElement>): void {
    drag.current = null;
    if (event.currentTarget.hasPointerCapture(event.pointerId))
      event.currentTarget.releasePointerCapture(event.pointerId);
  }
  /** 分隔条可用键盘调整，不要求鼠标拖动。 */
  function resizeKey(event: KeyboardEvent<HTMLDivElement>): void {
    if (event.key !== "ArrowUp" && event.key !== "ArrowDown") return;
    event.preventDefault();
    setHeight((old) => bounded(old + (event.key === "ArrowUp" ? 20 : -20)));
  }
  /** 标签方向键导航并同步 DOM 焦点。 */
  function tabKey(event: KeyboardEvent<HTMLDivElement>): void {
    if (event.key === "Escape") {
      setMenu(false);
      return;
    }
    if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
    event.preventDefault();
    const next = moveBottomTab(state, event.key === "ArrowRight" ? 1 : -1);
    setState(next);
    panel.current
      ?.querySelector<HTMLButtonElement>(`[data-bottom-tab="${next.activeId}"]`)
      ?.focus();
  }
  return {
    ...state,
    panel,
    menu,
    height,
    maximumHeight,
    addOperations,
    addTerminal,
    terminalBusy,
    terminalMessage,
    pendingClose,
    quitPending,
    cancelClose,
    confirmClose,
    select,
    close,
    toggleMenu,
    startResize,
    resize,
    endResize,
    resizeKey,
    tabKey,
  };
}
