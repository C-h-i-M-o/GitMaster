import assert from "node:assert/strict";
import test from "node:test";
import { gitTools, gitWriteError } from "./git-tools.ts";

/** 严格 schema 拒绝路径和参数注入，写失败与结果不明不得变成成功。 */
test("Git 工具边界和写结果真实投影", () => {
  assert.equal(gitTools.length, 13);
  assert.equal(
    gitTools.filter((tool) => "write" in tool && tool.write).length,
    5,
  );
  const stage = gitTools.find((tool) => tool.name === "git_stage")!;
  assert.equal(
    stage.schema.safeParse({ paths: ["../outside"] }).success,
    false,
  );
  assert.equal(
    stage.schema.safeParse({ paths: ["明确文件.txt"], all: true }).success,
    false,
  );
  assert.equal(gitWriteError({ result: { outcome: "succeeded" } }), null);
  assert.equal(
    gitWriteError({
      result: { outcome: "failed", error: { code: "STALE_WRITE_PLAN" } },
    }),
    "STALE_WRITE_PLAN",
  );
  assert.equal(
    gitWriteError({
      result: { outcome: "unknown", error: { code: "TIMEOUT" } },
    }),
    "WRITE_OUTCOME_UNKNOWN",
  );
  assert.equal(gitWriteError({ ok: true }), "AI_PROTOCOL");
});
