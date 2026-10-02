import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent,
} from "react";

/** 二级差异覆盖时隔离被遮挡控件，并提供键盘返回文件列表的路径。 */
export function useChangeDetailFocus(open: boolean, close: () => void) {
  const list = useRef<HTMLElement>(null);
  const detail = useRef<HTMLElement>(null);
  const [narrow, setNarrow] = useState(
    () => matchMedia("(max-width: 1150px)").matches,
  );
  const previous = useRef(false);
  useEffect(() => {
    const media = matchMedia("(max-width: 1150px)");
    /** 响应窗口宽度变化，宽屏保持两层均可交互。 */
    function changed(): void {
      setNarrow(media.matches);
    }
    changed();
    media.addEventListener("change", changed);
    return () => media.removeEventListener("change", changed);
  }, []);
  useLayoutEffect(() => {
    if (open && (!previous.current || narrow)) {
      detail.current
        ?.querySelector<HTMLButtonElement>("button")
        ?.focus({ preventScroll: true });
    } else if (!open && previous.current) {
      list.current
        ?.querySelector<HTMLElement>('[role="grid"]')
        ?.focus({ preventScroll: true });
    }
    previous.current = open;
  }, [open, narrow]);
  /** Escape 只关闭当前差异层，不丢失文件选择和提交草稿。 */
  function keyDown(event: KeyboardEvent<HTMLElement>): void {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      close();
    }
  }
  return { list, detail, covered: open && narrow, keyDown };
}
