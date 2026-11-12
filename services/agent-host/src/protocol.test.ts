import assert from "node:assert/strict";
import { test } from "node:test";
import { FrameDecoder } from "./protocol.ts";

test("私有协议按换行分帧并正确重组分片中文", () => {
  const decoder = new FrameDecoder();
  const frame = Buffer.from(
    '{"jsonrpc":"2.0","id":1,"method":"read","params":{"text":"中文"}}\n',
  );
  assert.deepEqual(decoder.feed(frame.subarray(0, frame.length - 5)), []);
  assert.equal(decoder.feed(frame.subarray(frame.length - 5)).length, 1);
});

test("拒绝超长未结束帧，不能无限积累子进程输出", () => {
  assert.throws(
    () => new FrameDecoder().feed(Buffer.alloc(262145, 97)),
    /AI_FRAME_TOO_LARGE/,
  );
});
