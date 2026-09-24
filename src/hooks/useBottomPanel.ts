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

/** 管理底部标签和面板尺寸；折叠或切标签不卸载记录视图。 */
export function useBottomPanel(expanded: boolean, toggleExpanded: () => void) {
  const [state, setState] = useState<BottomTabsState>({
    tabs: [{ id: "operations-1", kind: "operations", title: "操作记录" }],
    activeId: "operations-1",
  });
  const sequence = useRef(1);
  const panel = useRef<HTMLElement>(null);
  const [menu, setMenu] = useState(false);
  const [height, setHeight] = useState(300);
  const [maximumHeight, setMaximumHeight] = useState(410);
  useEffect(() => {
    const parent = panel.current?.parentElement;
    if (!parent) return;
    /** 可用空间变小时收回面板，并同步辅助功能的数值范围。 */
    function measure(): void {
      const maximum = Math.max(120, (parent?.clientHeight ?? 650) - 240);
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
  /** 标签激活只改变可见项，不清除其筛选或滚动。 */
  function select(id: string): () => void {
    return () => {
      setState((old) => ({ ...old, activeId: id }));
      if (!expanded) toggleExpanded();
    };
  }
  /** 关闭标签后仍保留操作控制器中的任务。 */
  function close(id: string): () => void {
    return () => setState((old) => closeBottomTab(old, id));
  }
  /** 显式打开新建菜单，不触发其他标签动作。 */
  function toggleMenu(): void {
    setMenu((value) => !value);
  }
  /** 根据可用工作台高度限制面板尺寸，保持图和操作栏可达。 */
  function bounded(value: number): number {
    return Math.min(
      Math.max(120, (panel.current?.parentElement?.clientHeight ?? 650) - 240),
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
