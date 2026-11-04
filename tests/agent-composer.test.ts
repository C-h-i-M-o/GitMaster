import assert from "node:assert/strict";
import test from "node:test";
import {
  draftAfterSend,
  handleComposerKeyDown,
} from "../src/ui/agentComposer.ts";

/** 用可编辑 textarea 夹具执行与 React 键盘回调相同的按键处理函数。 */
function press(
  overrides: Record<string, unknown> = {},
  options: {
    value?: string;
    start?: number;
    end?: number;
    canSend?: boolean;
    enterHeld?: boolean;
  } = {},
) {
  const target = {
    value: options.value ?? "hello",
    selectionStart: options.start ?? 5,
    selectionEnd: options.end ?? 5,
    setRangeText(replacement: string, start: number, end: number) {
      this.value = `${this.value.slice(0, start)}${replacement}${this.value.slice(end)}`;
      this.selectionStart = this.selectionEnd = start + replacement.length;
    },
  };
  let prevented = false;
  let sends = 0;
  let changed = "";
  const result = handleComposerKeyDown(
    {
      key: "Enter",
      ctrlKey: false,
      metaKey: false,
      shiftKey: false,
      repeat: false,
      nativeEvent: { isComposing: false, keyCode: 13 },
      currentTarget: target,
      preventDefault: () => {
        prevented = true;
      },
      ...overrides,
    } as Parameters<typeof handleComposerKeyDown>[0],
    {
      enterHeld: options.enterHeld ?? false,
      canSend: options.canSend ?? true,
      send: () => sends++,
      changeDraft: (value) => (changed = value),
    },
  );
  return { result, target, prevented, sends, changed };
}

test("Enter 发送；Ctrl/Command/Shift+Enter 在选区插入换行", () => {
  const sent = press();
  assert.equal(sent.result, "send");
  assert.equal(sent.prevented, true);
  assert.equal(sent.sends, 1);
  for (const modifier of ["ctrlKey", "metaKey", "shiftKey"]) {
    const inserted = press(
      { [modifier]: true },
      { value: "abcd", start: 2, end: 3 },
    );
    assert.equal(inserted.result, "newline");
    assert.equal(inserted.target.value, "ab\nd");
    assert.equal(inserted.target.selectionStart, 3);
    assert.equal(inserted.changed, "ab\nd");
    assert.equal(inserted.sends, 0);
  }
});

test("IME、长按重复和运行中的 Enter 不会误发送", () => {
  const composition = press({
    nativeEvent: { isComposing: true, keyCode: 13 },
  });
  assert.equal(composition.result, "ignore");
  assert.equal(composition.prevented, false);
  const legacyComposition = press({
    nativeEvent: { isComposing: false, keyCode: 229 },
  });
  assert.equal(legacyComposition.result, "ignore");
  const repeated = press({ repeat: true });
  assert.equal(repeated.result, "ignore");
  assert.equal(repeated.prevented, true);
  assert.equal(repeated.target.value, "hello");
  const held = press({}, { enterHeld: true });
  assert.equal(held.result, "ignore");
  assert.equal(held.prevented, true);
  assert.equal(held.target.value, "hello");
  const running = press({}, { canSend: false });
  assert.equal(running.result, "newline");
  assert.equal(running.sends, 0);
});

test("发送失败保留原草稿，成功时不覆盖用户已编辑的下一条", () => {
  assert.equal(draftAfterSend("原消息", "原消息", false), "原消息");
  assert.equal(draftAfterSend("下一条", "原消息", true), "下一条");
  assert.equal(draftAfterSend("原消息", "原消息", true), "");
});
