/** 处理输入区按键所需的最小事件接口。 */
export interface ComposerKeyboardEvent {
  key: string;
  ctrlKey: boolean;
  metaKey: boolean;
  shiftKey: boolean;
  repeat: boolean;
  nativeEvent: { isComposing: boolean; keyCode: number };
  currentTarget: {
    value: string;
    selectionStart: number;
    selectionEnd: number;
    setRangeText(
      replacement: string,
      start: number,
      end: number,
      selectionMode: "end",
    ): void;
  };
  preventDefault(): void;
}

/** 按真实输入事件插入换行或发送，避免依赖浏览器修饰键默认行为。 */
export function handleComposerKeyDown(
  event: ComposerKeyboardEvent,
  options: {
    enterHeld: boolean;
    canSend: boolean;
    send(): void;
    changeDraft(value: string): void;
  },
): "send" | "newline" | "ignore" {
  if (event.key !== "Enter") return "ignore";
  if (event.nativeEvent.isComposing || event.nativeEvent.keyCode === 229)
    return "ignore";
  if (event.repeat || options.enterHeld) {
    event.preventDefault();
    return "ignore";
  }
  event.preventDefault();
  if (event.ctrlKey || event.metaKey || event.shiftKey || !options.canSend) {
    const { selectionStart, selectionEnd } = event.currentTarget;
    event.currentTarget.setRangeText("\n", selectionStart, selectionEnd, "end");
    options.changeDraft(event.currentTarget.value);
    return "newline";
  }
  options.send();
  return "send";
}

/** 只在发送成功且草稿未被用户更新时清空，失败或新草稿都保留。 */
export function draftAfterSend(
  currentDraft: string,
  submittedDraft: string,
  succeeded: boolean,
): string {
  return succeeded && currentDraft === submittedDraft ? "" : currentDraft;
}
