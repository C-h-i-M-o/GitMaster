import assert from "node:assert/strict";
import test from "node:test";
import { AgentChatMemory, titleValidation } from "../src/ui/agentChatMemory.ts";
import type { AgentEvent, ChatRecord } from "../src/types/agent.ts";

/** 构造带真实序列的持久聊天快照。 */
function record(id: string, sequences: number[]): ChatRecord {
  return { session: { version: 1, id, title: "测试", repositoryPath: null, profileId: "p", permissionMode: "requestApproval", archived: false, createdAt: 0, updatedAt: 0 }, events: sequences.map(sequence => ({ version: 1, sequence, sessionId: id, runId: "r", timestamp: 0, kind: "userMessage", text: String(sequence) })) };
}

test("旧快照返回前到达的事件按序列合并且不重复", () => {
  const memory = new AgentChatMemory();
  memory.beginLoad("a");
  const event: AgentEvent = record("a", [2]).events[0]!;
  memory.receive(event);
  memory.receive(event);
  memory.receive(record("b", [9]).events[0]!);
  const loaded = memory.merge(record("a", [1]), null);
  assert.deepEqual(loaded.events.map(event => event.sequence), [1, 2]);
  assert.deepEqual(memory.merge(record("a", [1, 2]), loaded).events.map(event => event.sequence), [1, 2]);
});

test("聊天和未建立聊天的项目草稿相互隔离且可以恢复", () => {
  const memory = new AgentChatMemory();
  memory.setDraft("a", "/one", "A 草稿");
  assert.equal(memory.draft("b", "/one"), "");
  memory.setDraft("b", "/one", "B 草稿");
  assert.equal(memory.draft("a", "/two"), "A 草稿");
  memory.setDraft(null, "/one", "新聊天");
  assert.equal(memory.draft(null, "/two"), "");
  assert.equal(memory.draft(null, "/one"), "新聊天");
  memory.setDraft("a", "/one", "");
  assert.equal(memory.draft("b", "/one"), "B 草稿");
});

test("标题按 Unicode 字符统一 120 字限制", () => {
  for (const character of ["中", "😀"]) {
    assert.equal(titleValidation(character.repeat(120)).error, null);
    assert.equal(titleValidation(character.repeat(121)).count, 121);
    assert.ok(titleValidation(character.repeat(121)).error);
  }
  assert.ok(titleValidation("   ").error);
});
