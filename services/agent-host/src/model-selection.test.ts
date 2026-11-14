import assert from "node:assert/strict";
import test from "node:test";
import { chooseAiModel } from "../../../src/ui/aiModelSelection.ts";

const first = {
  providerId: "fixture",
  modelId: "first",
  reasoningEffort: null,
};
const other = { ...first, modelId: "other" };

/** 首次配置后新会话可直接使用模型，空配置仍不可发送。 */
test("新会话没有默认模型时选择首个可用模型", () => {
  assert.deepEqual(chooseAiModel(null, null, null, first), first);
  assert.equal(chooseAiModel(null, null, null, null), null);
});

/** 保持显式、历史和默认选择优先级，失效引用也不得静默替换。 */
test("明确选择、会话和默认模型优先于首个模型", () => {
  assert.deepEqual(chooseAiModel(other, first, first, first), other);
  assert.deepEqual(chooseAiModel(null, other, first, first), other);
  assert.deepEqual(chooseAiModel(null, null, other, first), other);
});
