import { useEffect, useRef, type RefObject, type SyntheticEvent } from "react";

export interface NativeDialogControls {
  dialogRef: RefObject<HTMLDialogElement | null>;
  onCancel: (event: SyntheticEvent<HTMLDialogElement>) => void;
}

/** 管理原生 dialog 的显示、Esc 关闭和关闭后的焦点恢复。 */
export function useNativeDialog(
  open: boolean,
  onClose: () => void,
): NativeDialogControls {
  const dialogRef = useRef<HTMLDialogElement | null>(null);
  const previousFocus = useRef<HTMLElement | null>(null);

  useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog) return;
    if (open) {
      previousFocus.current =
        document.activeElement instanceof HTMLElement
          ? document.activeElement
          : null;
      if (!dialog.open) dialog.showModal();
      dialog
        .querySelector<HTMLElement>(
          "button, [href], input, select, textarea, [tabindex]:not([tabindex='-1'])",
        )
        ?.focus();
      return;
    }
    if (dialog.open) dialog.close();
    previousFocus.current?.focus();
    previousFocus.current = null;
  }, [open]);

  /** 将原生 cancel 事件转为调用方的关闭动作。 */
  const onCancel = (event: SyntheticEvent<HTMLDialogElement>): void => {
    event.preventDefault();
    onClose();
  };
  return { dialogRef, onCancel };
}
