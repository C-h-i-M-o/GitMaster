import assert from "node:assert/strict";
import test from "node:test";
import { createServer } from "vite";

test("引用数量和 UTF-8 字节数受前端上限约束", async () => {
  const server = await createServer({
    server: { middlewareMode: true, hmr: false },
    appType: "custom",
  });
  try {
    const helpers = await server.ssrLoadModule("/src/ui/agentContext.ts");
    const eightReferences = Array.from({ length: 8 }, (_, index) => ({
      kind: "file",
      path: "src/" + index + ".ts",
    }));
    assert.equal(helpers.referenceBatchError(eightReferences), null);
    assert.match(
      helpers.referenceBatchError([
        ...eightReferences,
        { kind: "text", text: "a" },
      ]),
      /最多选择 8 项/,
    );
    assert.match(
      helpers.referenceBatchError([{ kind: "text", text: "界".repeat(22000) }]),
      /64 KiB/,
    );
  } finally {
    await server.close();
  }
});

test("引用预览版本回绑到原请求并拒绝响应数量不匹配", async () => {
  const server = await createServer({
    server: { middlewareMode: true, hmr: false },
    appType: "custom",
  });
  try {
    const helpers = await server.ssrLoadModule("/src/ui/agentContext.ts");
    const refs = [
      { kind: "file", path: "src/main.ts", startLine: 2, endLine: 4 },
    ];
    assert.deepEqual(
      helpers.bindReferenceVersions(refs, [
        {
          kind: "file",
          label: "src/main.ts",
          path: "src/main.ts",
          startLine: 2,
          endLine: 4,
          version: "sha256:verified",
          text: "内容",
        },
      ]),
      [{ ...refs[0], expectedVersion: "sha256:verified" }],
    );
    assert.equal(helpers.bindReferenceVersions(refs, []), null);
    assert.equal(
      helpers.bindReferenceVersions(refs, [
        {
          kind: "file",
          label: "other.ts",
          path: "src/other.ts",
          startLine: 2,
          endLine: 4,
          version: "sha256:other",
          text: "内容",
        },
      ]),
      null,
    );
  } finally {
    await server.close();
  }
});

test("切换工作区或聊天时使引用校验忙状态复位，仅创建聊天保留草稿引用", async () => {
  const server = await createServer({
    server: { middlewareMode: true, hmr: false },
    appType: "custom",
  });
  try {
    const helpers = await server.ssrLoadModule("/src/ui/agentContext.ts");
    const previous = { workspaceId: "workspace-a", chatId: "chat-a" };
    const switched = helpers.contextScopeTransition(
      previous,
      { workspaceId: "workspace-b", chatId: "chat-b" },
      false,
    );
    assert.equal(switched.clearReferenceBusy, true);
    assert.equal(switched.clearContextBusy, true);
    assert.equal(switched.preserveReferences, false);
    const created = helpers.contextScopeTransition(
      { workspaceId: "workspace-a", chatId: null },
      { workspaceId: "workspace-a", chatId: "chat-new" },
      true,
    );
    assert.equal(created.preserveReferences, true);
    assert.equal(created.clearReferenceBusy, true);
    assert.equal(created.clearContextBusy, true);
  } finally {
    await server.close();
  }
});

test("模型、参数或记忆变更会改变上下文预览请求键", async () => {
  const server = await createServer({
    server: { middlewareMode: true, hmr: false },
    appType: "custom",
  });
  try {
    const helpers = await server.ssrLoadModule("/src/ui/agentContext.ts");
    const base = helpers.agentContextRequestKey(
      "workspace-a",
      "chat-a",
      "输入",
      [],
      "model-rev-1",
      0,
    );
    assert.notEqual(
      base,
      helpers.agentContextRequestKey(
        "workspace-a",
        "chat-a",
        "输入",
        [],
        "model-rev-2",
        0,
      ),
    );
    assert.notEqual(
      base,
      helpers.agentContextRequestKey(
        "workspace-a",
        "chat-a",
        "输入",
        [],
        "model-rev-1",
        1,
      ),
    );
  } finally {
    await server.close();
  }
});
