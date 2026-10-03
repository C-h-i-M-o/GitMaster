import assert from "node:assert/strict";
import test from "node:test";
import {
  createOperationsController,
  type OperationsApi,
} from "../src/hooks/operationsController.ts";
import type { OperationRecord, RepositoryState } from "../src/types/git.ts";

const repository: RepositoryState = {
  repositoryId: "repo",
  snapshotId: "snap",
  rootPath: "/repo",
  head: { kind: "branch", name: "main", oid: "a" },
  operations: [],
  changes: [],
};
const error = { code: "GIT_EXECUTION_FAILED", retryable: false };
function fixture(): {
  controller: ReturnType<typeof createOperationsController>;
  api: OperationsApi;
} {
  const api = {
    prepareFileSave: async () => {
      throw error;
    },
    prepareLocalWrite: async () => {
      throw error;
    },
    prepareRemoteWrite: async () => {
      throw error;
    },
    prepareConflictWrite: async () => {
      throw error;
    },
    prepareClone: async () => {
      throw error;
    },
    executeWrite: async () => {
      throw error;
    },
    executeClone: async () => {
      throw error;
    },
    readOperation: async () => null,
  } as OperationsApi;
  const controller = createOperationsController(api);
  controller.setRepository(repository, true);
  return { controller, api };
}

test("准备失败为本地、远端、clone、finishMerge写入摘要并通知", async () => {
  for (const action of [
    (c: ReturnType<typeof createOperationsController>) =>
      c.prepareLocal({ kind: "stage", changeIds: [] }),
    (c: ReturnType<typeof createOperationsController>) =>
      c.prepareRemote({ kind: "fetchAll", remoteId: "r" }),
    (c: ReturnType<typeof createOperationsController>) =>
      c.prepareClone({
        directoryName: "/clone",
        remoteUrl: "https://example.invalid/repo",
      }),
    (c: ReturnType<typeof createOperationsController>) =>
      c.prepareConflict("session", { kind: "finishMerge", message: "完成" }),
  ]) {
    const { controller } = fixture();
    await action(controller);
    const state = controller.getSnapshot();
    assert.equal(state.history[0]?.outcome, "failed");
    assert.equal(state.failureNotification?.sequence, 0);
    controller.deactivate();
  }
});

test("saveFile和saveConflict准备失败不进入历史或通知", async () => {
  const { controller } = fixture();
  await controller.saveFile("file", "v", "内容");
  assert.equal(controller.getSnapshot().operationScope?.kind, "saveFile");
  assert.equal(controller.getSnapshot().history.length, 0);
  assert.equal(controller.getSnapshot().failureNotification, null);
  await controller.prepareConflict("session", {
    kind: "saveConflict",
    conflictId: "c",
    fingerprint: "f",
    content: "内容",
  });
  assert.equal(controller.getSnapshot().history.length, 0);
  assert.equal(controller.getSnapshot().failureNotification, null);
});

test("相同终态重复读取不重复通知", async () => {
  const { controller, api } = fixture();
  const record: OperationRecord = {
    progress: {
      handle: { operationId: "failed-stage", repositoryId: "repo" },
      sequence: 3,
      kind: "stage",
      phase: "failed",
      counts: null,
      startedAt: 1,
    },
    result: {
      outcome: "failed",
      operationId: "failed-stage",
      kind: "stage",
      error,
      refresh: { status: "notApplicable" },
    },
  };
  api.readOperation = async () => record;
  await controller.resume();
  const first = controller.getSnapshot().failureNotification;
  assert.equal(first?.id, "failed-stage");
  assert.equal(controller.getSnapshot().history.length, 1);
  await controller.resume();
  assert.equal(controller.getSnapshot().failureNotification, first);
  assert.equal(controller.getSnapshot().history.length, 1);
  controller.deactivate();
});

test("成功不通知，成功后的核实失败单独通知", async () => {
  for (const failedRefresh of [false, true]) {
    const { controller, api } = fixture();
    api.readOperation = async () => ({
      progress: {
        handle: { operationId: "commit", repositoryId: "repo" },
        sequence: 2,
        kind: "commit",
        phase: "completed",
        counts: null,
        startedAt: 1,
      },
      result: {
        outcome: "succeeded",
        operationId: "commit",
        kind: "commit",
        commitOid: "a",
        branchName: null,
        clonePath: null,
        refresh: failedRefresh
          ? { status: "failed", error }
          : { status: "ready", state: repository },
      },
    });
    await controller.resume();
    assert.equal(controller.getSnapshot().history[0]?.outcome, "succeeded");
    assert.equal(
      controller.getSnapshot().failureNotification?.id ?? null,
      failedRefresh ? "commit" : null,
    );
    assert.equal(
      controller.getSnapshot().history[0]?.refreshError?.code ?? null,
      failedRefresh ? error.code : null,
    );
    controller.deactivate();
  }
});

test("相同错误的两次用户准备失败生成不同摘要ID", async () => {
  const { controller } = fixture();
  await controller.prepareLocal({ kind: "stage", changeIds: [] });
  const first = controller.getSnapshot().history[0]?.id;
  await new Promise((resolve) => setTimeout(resolve, 1));
  await controller.prepareLocal({ kind: "stage", changeIds: [] });
  assert.notEqual(controller.getSnapshot().history[0]?.id, first);
});

test("切换项目后清理上个项目的错误横幅，但保留历史记录", async () => {
  const { controller } = fixture();
  await controller.prepareLocal({ kind: "stage", changeIds: [] });
  assert.ok(controller.getSnapshot().error);
  controller.setRepository(
    { ...repository, repositoryId: "second", snapshotId: "second-snap" },
    true,
  );
  assert.equal(controller.getSnapshot().error, null);
  assert.equal(controller.getSnapshot().history[0]?.repositoryId, "repo");
  controller.deactivate();
});
