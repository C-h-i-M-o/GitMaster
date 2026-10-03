import assert from "node:assert/strict";
import test from "node:test";
import {
  workspaceNotices,
  type WorkspaceNoticeInput,
} from "../src/ui/workspaceNotices.ts";

/** 独立状态输入用于核对错误优先级，不触发真实仓库操作。 */
function input(): WorkspaceNoticeInput {
  return {
    sync: {
      busy: false,
      phaseLabel: "同步",
      notice: null,
      diverged: false,
      error: null,
    },
    refresh: {
      busy: false,
      notice: null,
      resultLabel: null,
      error: null,
      networkStatus: "idle",
      localStatus: "idle",
    },
    errors: [],
    stale: false,
    activity: "idle",
  };
}
test("成功摘要和旧快照不能遮盖失败，重复错误仅呈现一次", () => {
  const value = input();
  const error = { code: "ACCESS_DENIED", retryable: false };
  value.errors = [error, error];
  value.sync.notice = "同步完成";
  value.stale = true;
  const notices = workspaceNotices(value);
  assert.equal(notices.filter((item) => item.tone === "error").length, 1);
  assert.equal(notices[0]?.tone, "error");
  assert.ok(notices.some((item) => item.id === "stale"));
  assert.ok(notices.some((item) => item.tone === "success"));
});
test("远端失败、本地成功呈现部分失败摘要并保留核实入口", () => {
  const value = input();
  value.refresh = {
    ...value.refresh,
    networkStatus: "failed",
    localStatus: "succeeded",
    resultLabel: "远端获取：失败 · 本地获取：成功",
  };
  value.activity = "unverified";
  const notices = workspaceNotices(value);
  assert.equal(
    notices.find((item) => item.id.startsWith("refresh:"))?.tone,
    "warning",
  );
  assert.equal(
    notices.find((item) => item.id === "unverified")?.action,
    "resume",
  );
});
