import assert from "node:assert/strict";
import test from "node:test";
import {
  workspaceNotices,
  workbenchNoticeInput,
  type WorkspaceNoticeInput,
} from "../src/ui/workspaceNotices.ts";
import type { Workbench } from "../src/hooks/useWorkbench.ts";

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

/** 仅提供通知展示依赖的状态，避免触发真实 IPC 或 Git 操作。 */
function workbench(): Workbench {
  const value = input();
  return {
    syncRemote: value.sync,
    manualRefresh: value.refresh,
    actionError: null,
    resourceError: null,
    drawer: null,
    changeDetailFocus: { covered: false },
    canOpen: true,
    openRecent: () => async () => {},
    repo: {
      repository: { repositoryId: "repo-a" },
      openingPath: null,
      loading: false,
      error: null,
      stale: false,
    },
    operations: {
      operationScope: { kind: "commit", repositoryId: "repo-a" },
      activity: "preparing",
      error: null,
      record: null,
      resume: async () => {},
    },
  } as Workbench;
}

/** 运行实际输入选择和通知生成链路，检查最终可见提示。 */
function noticesFor(w: Workbench) {
  return workspaceNotices(workbenchNoticeInput(w));
}

test("打开错误由局部卡片承接，其他错误及旧快照仍可见", () => {
  const w = workbench();
  w.repo.openingPath = "C:/fixture";
  w.repo.error = { code: "ACCESS_DENIED", retryable: true };
  w.repo.stale = true;
  w.actionError = { code: "INDEX_LOCKED", retryable: true };
  let notices = noticesFor(w);
  assert.equal(notices.filter((item) => item.tone === "error").length, 1);
  assert.match(notices[0]!.message, /锁定/);
  assert.ok(notices.some((item) => item.id === "stale"));
  w.repo.openingPath = null;
  notices = noticesFor(w);
  assert.equal(notices.filter((item) => item.tone === "error").length, 2);
});

test("提交局部反馈只承接摘要，具体错误仍在顶部保留", () => {
  const w = workbench();
  assert.ok(!noticesFor(w).some((item) => item.id === "preparing"));
  w.operations.activity = "unverified";
  w.operations.error = { code: "ACCESS_DENIED", retryable: true };
  for (const drawer of [null, "changes"] as const) {
    w.drawer = drawer;
    const notices = noticesFor(w);
    assert.ok(!notices.some((item) => item.id === "unverified"));
    assert.equal(notices.filter((item) => item.tone === "error").length, 1);
  }
});

test("切换到遮挡提交反馈的抽屉时，顶部承接结果核对入口", () => {
  const w = workbench();
  w.operations.activity = "unverified";
  for (const drawer of ["files", "conflicts", "detail"] as const) {
    w.drawer = drawer;
    assert.equal(
      noticesFor(w).find((item) => item.id === "unverified")?.action,
      "resume",
    );
  }
  w.drawer = null;
  w.repo.openingPath = "C:/another-fixture";
  w.repo.loading = true;
  assert.equal(
    noticesFor(w).find((item) => item.id === "unverified")?.action,
    "resume",
  );
});

test("窄窗口差异层遮挡局部提交反馈时，顶部保留恢复查询入口", () => {
  const w = workbench();
  w.drawer = "changes";
  w.operations.activity = "unverified";
  w.changeDetailFocus.covered = true;
  assert.equal(
    noticesFor(w).find((item) => item.id === "unverified")?.action,
    "resume",
  );
  w.changeDetailFocus.covered = false;
  assert.ok(!noticesFor(w).some((item) => item.id === "unverified"));
});

test("其他仓库任务不串入当前通知，非提交任务不被误隐藏", () => {
  const w = workbench();
  w.operations.operationScope!.repositoryId = "repo-b";
  w.operations.error = { code: "ACCESS_DENIED", retryable: true };
  assert.deepEqual(noticesFor(w), []);
  w.operations.operationScope = { kind: "stage", repositoryId: "repo-a" };
  assert.ok(noticesFor(w).some((item) => item.id === "preparing"));
});

test("不同错误对象即使中文相同也不互相覆盖", () => {
  const value = input();
  value.errors = [
    { code: "ACCESS_DENIED", retryable: true },
    { code: "ACCESS_DENIED", retryable: true },
  ];
  const notices = workspaceNotices(value);
  assert.equal(notices.length, 2);
  assert.notEqual(notices[0]!.id, notices[1]!.id);
});
