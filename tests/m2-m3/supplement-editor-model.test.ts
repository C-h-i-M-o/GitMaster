import assert from "node:assert/strict";
import test from "node:test";
import { editorModelText, editorDiskText } from "../../src/ui/editorModelText.ts";

test("编辑模型逐字节保留原换行和无末尾换行", () => {
  for (const [source, ending] of [["a\r\nb", "crlf"], ["a\rb\r", "cr"], ["a\nb", "lf"], ["", "none"], ["文字", "none"]] as const) {
    const model = editorModelText(source);
    assert.ok(!model.includes("\r"));
    assert.equal(editorDiskText(model, ending), source);
  }
  assert.equal(editorDiskText("新增\n行\n", "crlf"), "新增\r\n行\r\n");
  assert.equal(editorDiskText("新增\n行", "none"), "新增\n行");
});
