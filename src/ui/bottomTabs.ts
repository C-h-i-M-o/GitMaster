export interface BottomTab {
  id: string;
  kind: "operations";
  title: string;
}
export interface BottomTabsState {
  tabs: BottomTab[];
  activeId: string | null;
}

/** 新标签使用稳定身份，切换不创建或销毁其他标签状态。 */
export function addBottomTab(
  state: BottomTabsState,
  tab: BottomTab,
): BottomTabsState {
  if (state.tabs.some((item) => item.id === tab.id))
    return { ...state, activeId: tab.id };
  return { tabs: [...state.tabs, tab], activeId: tab.id };
}
/** 关闭当前标签优先激活右侧，其次左侧，不影响后台任务。 */
export function closeBottomTab(
  state: BottomTabsState,
  id: string,
): BottomTabsState {
  const index = state.tabs.findIndex((item) => item.id === id);
  if (index < 0) return state;
  const tabs = state.tabs.filter((item) => item.id !== id);
  return {
    tabs,
    activeId:
      state.activeId === id
        ? (tabs[index]?.id ?? tabs[index - 1]?.id ?? null)
        : state.activeId,
  };
}
/** 标签方向键循环访问现有项，不产生不存在的活动标签。 */
export function moveBottomTab(
  state: BottomTabsState,
  delta: number,
): BottomTabsState {
  if (!state.tabs.length) return state;
  const index = Math.max(
    0,
    state.tabs.findIndex((item) => item.id === state.activeId),
  );
  const next =
    (((index + delta) % state.tabs.length) + state.tabs.length) %
    state.tabs.length;
  return { ...state, activeId: state.tabs[next]!.id };
}
