import { useEffect, useRef } from "react";

export interface WorkbenchShortcutActions {
  togglePanel: () => void;
  save: () => void;
  openProject: () => void;
  openSettings: () => void;
  focusSearch: () => void;
  dismiss: () => boolean;
}

/** 注册工作台桌面快捷键；编辑器与终端可通过 shouldHandle 保留自身快捷键。 */
export function useWorkbenchShortcuts(
  actions: WorkbenchShortcutActions,
  shouldHandle: (event: KeyboardEvent) => boolean = () => true,
): void {
  const actionsRef = useRef(actions);
  const shouldHandleRef = useRef(shouldHandle);
  actionsRef.current = actions;
  shouldHandleRef.current = shouldHandle;
  useEffect(() => {
    const handle = (event: KeyboardEvent): void => {
      if (
        event.defaultPrevented ||
        !shouldHandleRef.current(event) ||
        event.isComposing ||
        event.repeat ||
        event.altKey ||
        event.shiftKey
      )
        return;
      if (event.key === "Escape" && !event.metaKey && !event.ctrlKey) {
        if (actionsRef.current.dismiss()) event.preventDefault();
        return;
      }
      const isMac = /Mac|iPhone|iPad/.test(navigator.platform);
      const modifier = isMac
        ? event.metaKey && !event.ctrlKey
        : event.ctrlKey && !event.metaKey;
      if (!modifier) return;
      const key = event.key.toLowerCase();
      const action =
        key === "j"
          ? actionsRef.current.togglePanel
          : key === "s"
            ? actionsRef.current.save
            : key === "o"
              ? actionsRef.current.openProject
              : key === ","
                ? actionsRef.current.openSettings
                : key === "f"
                  ? actionsRef.current.focusSearch
                  : null;
      if (!action) return;
      event.preventDefault();
      action();
    };
    window.addEventListener("keydown", handle);
    const custom = (event: Event): void => {
      if (
        event instanceof CustomEvent &&
        event.detail instanceof KeyboardEvent &&
        shouldHandleRef.current(event.detail)
      )
        actionsRef.current.togglePanel();
    };
    window.addEventListener("gitmaster:toggle-panel", custom);
    return () => {
      window.removeEventListener("keydown", handle);
      window.removeEventListener("gitmaster:toggle-panel", custom);
    };
  }, []);
}
