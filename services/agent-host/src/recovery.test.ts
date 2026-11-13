import assert from "node:assert/strict";
import test from "node:test";
import {
  aiRequestIssue,
  aiRecoveryAction,
  mergeAiHistory,
} from "../../../src/ui/aiRecovery.ts";
import type { AiEventEnvelope } from "./contracts.ts";

/** 合成轮次只包含状态，不使用真实用户会话。 */
function event(
  sequence: number,
  turn: number,
  value: AiEventEnvelope["event"],
  epoch = "epoch",
): AiEventEnvelope {
  return {
    sequence,
    turn,
    runtimeEpoch: epoch,
    sessionId: "session",
    runId: null,
    event: value,
  };
}
const failure = {
  kind: "error",
  code: "AI_NETWORK_ERROR",
  message: "连接超时（UND_ERR_CONNECT_TIMEOUT）",
} as const;

/** 早期失败保留在时间线，更晚成功不再显示当前连接故障。 */
test("历史失败后成功及重复重载不产生当前故障", () => {
  const rows = [
    event(1, 1, failure),
    event(2, 1, { kind: "turn", turn: 1, state: "failed" }),
    event(3, 2, { kind: "turn", turn: 2, state: "completed" }),
  ];
  const merged = mergeAiHistory(rows, rows, "session", "epoch");
  assert.equal(merged.length, 3);
  assert.equal(merged[0]?.event.kind, "error");
  assert.equal(aiRequestIssue(merged, null), null);
});

/** 最后一轮失败只表达上轮事实，实际活动轮次才显示当前请求失败。 */
test("末轮错误与当前运行标识准确区分", () => {
  const row = { ...event(1, 2, failure), runId: "active" };
  const rows = [row, event(2, 2, { kind: "turn", turn: 2, state: "failed" })];
  assert.equal(aiRequestIssue(rows, null)?.title, "上一轮请求失败");
  assert.equal(aiRequestIssue(rows, "active")?.title, "本次请求失败");
  assert.equal(aiRecoveryAction(aiRequestIssue(rows, null)!), "connect");
});

/** 新轮次开始后不把旧失败带入进行中的请求。 */
test("新轮次开始及取消不恢复过期错误", () => {
  assert.equal(
    aiRequestIssue(
      [
        event(1, 1, failure),
        event(2, 2, { kind: "turn", turn: 2, state: "running" }),
      ],
      "active",
    ),
    null,
  );
  assert.equal(
    aiRequestIssue(
      [
        event(1, 1, failure),
        event(2, 1, { kind: "turn", turn: 1, state: "cancelled" }),
      ],
      null,
    ),
    null,
  );
});

/** 历史页与实时补读交错时按代次与序号合并，隔离其他会话。 */
test("历史合并保留实时新事件并拒绝旧代次及跨会话事件", () => {
  const old = event(1, 1, failure, "old");
  const page = event(2, 1, { kind: "turn", turn: 1, state: "running" });
  const completed = event(3, 1, { kind: "turn", turn: 1, state: "completed" });
  assert.deepEqual(
    mergeAiHistory(
      [old, page],
      [completed, page, { ...old, sessionId: "other" }],
      "session",
      "epoch",
    ).map((row) => row.sequence),
    [2, 3],
  );
});

/** 旧宿主缺少运行时代次时明确拒绝，不能静默显示空白记录。 */
test("历史读取缺失运行时代次时返回协议错误", () => {
  assert.throws(() => mergeAiHistory([], [], "session", ""), {
    code: "AI_PROTOCOL",
  });
});

/** 操作失败提供匹配动作，配置错误不通过历史重载伪装修复。 */
test("错误来源映射到实际可执行的恢复动作", () => {
  const base = {
    scope: "runtime" as const,
    code: "AI_RUNTIME_STOPPED",
    message: "宿主已结束",
    title: "会话连接异常",
    sessionId: "session",
    runtimeEpoch: null,
    runId: null,
    turn: null,
  };
  assert.equal(aiRecoveryAction(base), "reload");
  assert.equal(aiRecoveryAction({ ...base, scope: "model-request" }), "reload");
  assert.equal(aiRecoveryAction({ ...base, scope: "history" }), "reload");
  assert.equal(aiRecoveryAction({ ...base, scope: "job" }), "reload");
  assert.equal(
    aiRecoveryAction({ ...base, scope: "job", code: "AI_JOB_READ_FAILED" }),
    "jobs",
  );
  assert.equal(
    aiRecoveryAction({ ...base, scope: "model-request", code: "AI_HTTP_401" }),
    "settings",
  );
  assert.equal(
    aiRecoveryAction({
      ...base,
      scope: "model-request",
      code: "AI_MODEL_ERROR",
    }),
    null,
  );
  assert.equal(aiRecoveryAction({ ...base, code: "AI_BUSY" }), null);
  assert.equal(aiRecoveryAction({ ...base, code: "AI_UNSAVED_FILES" }), null);
  assert.equal(
    aiRecoveryAction({ ...base, code: "AI_DECISION_EXPIRED" }),
    null,
  );
  assert.equal(
    aiRecoveryAction({ ...base, code: "AI_UNKNOWN_MODEL" }),
    "settings",
  );
});
