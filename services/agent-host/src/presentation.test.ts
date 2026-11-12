import assert from "node:assert/strict";
import test from "node:test";
import { aiProcessPolicy } from "../../../src/ui/aiProcessPolicy.ts";
import { buildAiTurns } from "../../../src/ui/aiTurns.ts";

/** 四档与两时机覆盖失败、中断和普通完成，避免折叠隐藏需要处理的信息。 */
test("过程展示沿用 DSH 行为矩阵", () => {
  for (const transcriptView of [
    "compact",
    "standard",
    "detailed",
    "verbose",
  ] as const)
    for (const collapseTiming of ["completion", "next-input"] as const) {
      const preferences = { transcriptView, collapseTiming };
      for (const state of [
        "running",
        "failed",
        "cancelled",
        "interrupted",
      ] as const)
        assert.equal(aiProcessPolicy(preferences, state, true).foldTurn, false);
      assert.equal(
        aiProcessPolicy(preferences, "completed", false).foldTurn,
        transcriptView !== "verbose" && collapseTiming === "completion",
      );
      assert.equal(
        aiProcessPolicy(preferences, "completed", true).foldTurn,
        transcriptView !== "verbose",
      );
      assert.equal(
        aiProcessPolicy(preferences, "running", false).stepGrouping,
        transcriptView === "verbose"
          ? "none"
          : transcriptView === "detailed"
            ? "history"
            : "collapsed",
      );
    }
});

/** 正常完成按时机收起，失败和悬空工具保持可见，不能把打字当成下一条输入。 */
test("真实轮次投影保留全部回复并结束悬空工具", () => {
  const rows = [
    {
      key: "user",
      turn: 1,
      step: 1,
      event: { kind: "user" as const, text: "问题" },
    },
    {
      key: "tool",
      turn: 1,
      step: 1,
      event: {
        kind: "tool" as const,
        toolCallId: "t",
        name: "pwsh",
        input: "{}",
        output: null,
        failed: false,
      },
    },
    {
      key: "reply",
      turn: 1,
      step: 1,
      event: {
        kind: "assistant" as const,
        messageId: "a",
        text: "回复",
        final: true,
      },
    },
    {
      key: "end",
      turn: 1,
      step: 1,
      event: { kind: "turn" as const, turn: 1, state: "interrupted" as const },
    },
  ];
  const turn = buildAiTurns(
    rows,
    { transcriptView: "standard", collapseTiming: "completion" },
    false,
  )[0]!;
  assert.equal(turn.policy.foldTurn, false);
  assert.equal(turn.messages.length, 2);
  const tool = turn.process[0]?.event;
  assert.equal(tool?.kind, "tool");
  if (tool?.kind === "tool") assert.equal(tool.state, "interrupted");
});
