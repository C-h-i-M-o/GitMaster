import assert from "node:assert/strict";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { homedir, tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import {
  LlmAdapter,
  MessageId,
  ToolCallId,
  type GenerateOptions,
  type StreamChunk,
} from "@deepseek-ai/dsh-llm";
import { SessionId } from "@deepseek-ai/dsh-session";
import { createRuntime } from "./runtime.ts";
import { createHash } from "node:crypto";

/** 内置技能独立于用户目录；上游自动注册的 ACL 修复技能不得被模型或用户手势调用。 */
test("内置开发总结技能完整且 ACL 修复技能不可调用", async () => {
  const runtime = await createRuntime();
  try {
    const skill = await runtime.ctx.skills.get("git-development-summary");
    assert.ok(skill);
    assert.equal(
      createHash("sha256").update(skill.content).digest("hex").toUpperCase(),
      "02489AD081F2C1C93FBC8873ED25F365B37509639F5641EC4D4D7B0A47E70416",
    );
    assert.equal(skill.invocation.modelInvocable, true);
    assert.equal(skill.invocation.userInvocable, true);
    const blocked = await runtime.ctx.skills.get(
      "diagnose-windows-sandbox-acl",
    );
    assert.deepEqual(blocked?.invocation, {
      modelInvocable: false,
      userInvocable: false,
    });
  } finally {
    await runtime.ctx.fiber.dispose();
  }
});

/** 可控模型只请求读取临时文件，第二步必须看到真实工具结果。 */
class ReadFixtureAdapter extends LlmAdapter {
  calls = 0;
  /** 用上游流块驱动真实 Agent 循环，不伪造文件工具的执行结果。 */
  async *stream(options: GenerateOptions): AsyncIterable<StreamChunk> {
    this.calls += 1;
    if (this.calls === 1) {
      assert.deepEqual(
        options.tools?.map((tool) => tool.name).sort(),
        [
          "ask_user_question",
          "edit",
          "glob",
          "grep",
          "job_kill",
          "job_list",
          "job_output",
          process.platform === "win32" ? "pwsh" : "bash",
          "read",
          "skill",
          "write",
        ].sort(),
      );
      yield {
        type: "block-end",
        index: 0,
        block: {
          type: "tool-call",
          id: ToolCallId("read-1"),
          name: "read",
          arguments: { file_path: "fixture.txt" },
        },
      };
      yield { type: "finish", reason: "tool-calls" };
    } else {
      assert.match(JSON.stringify(options.messages), /临时文件中的真实内容/);
      yield {
        type: "block-end",
        index: 0,
        block: { type: "text", text: "文件读取成功" },
      };
      yield { type: "finish", reason: "stop" };
    }
  }
}

test(
  "精简运行时将真实文件读取结果回填模型并结束轮次",
  { timeout: 15000 },
  async () => {
    const root = await mkdtemp(join(tmpdir(), "gitmaster-agent-p0-"));
    const runtime = await createRuntime({ workspaceRoot: root });
    try {
      assert.ok(runtime.ctx.agents, "必须挂载上游 Agent 服务");
      await writeFile(
        join(root, "fixture.txt"),
        "临时文件中的真实内容",
        "utf8",
      );
      const adapter = new ReadFixtureAdapter();
      runtime.ctx.llm.registerAdapter(["fixture"], adapter);
      const handle = await runtime.ctx.agents.create({
        sessionId: SessionId("p0-session"),
        meta: { cwd: root },
        agentOptions: { provider: "fixture", model: "fixture" },
      });
      try {
        handle.agent.followup({
          role: "user",
          id: MessageId("p0-input"),
          content: [{ type: "text", text: "读取 fixture.txt 并汇报" }],
          source: { kind: "user" },
        });
        await handle.agent.whenIdle();
        assert.equal(adapter.calls, 2);
        assert.match(
          JSON.stringify(handle.agent.session.snapshotEvents()),
          /文件读取成功/,
        );
      } finally {
        await handle.dispose();
      }
    } finally {
      await runtime.ctx.fiber.dispose();
      await rm(root, { recursive: true, force: true });
    }
  },
);

test(
  "真实平台沙箱允许临时工作区写入并拒绝其他目录写入",
  { timeout: 120000 },
  async () => {
    const root = await mkdtemp(join(tmpdir(), "gitmaster-sandbox-p0-"));
    // 系统临时区属于允许写入范围，拒绝探针须放在该范围之外。
    const outside = await mkdtemp(join(homedir(), ".gitmaster-outside-p0-"));
    const runtime = await createRuntime({ workspaceRoot: root });
    try {
      const allowed = join(root, "allowed.txt");
      const denied = join(outside, "denied.txt");
      const insideCommand =
        process.platform === "win32"
          ? `Set-Content -LiteralPath '${allowed.replaceAll("'", "''")}' -Value '沙箱中文' -Encoding UTF8 -ErrorAction Stop`
          : `printf '沙箱中文' > '${allowed.replaceAll("'", "'\\''")}'`;
      const inside = await runtime.ctx.shell.execute(
        runtime.ctx.shell.resolve({ command: insideCommand, workdir: root }),
      );
      const insideResult = await inside.result();
      assert.equal(insideResult.exitCode, 0, JSON.stringify(insideResult));
      assert.match(await readFile(allowed, "utf8"), /沙箱中文/);
      const outsideCommand =
        process.platform === "win32"
          ? `Set-Content -LiteralPath '${denied.replaceAll("'", "''")}' -Value '不得写入' -ErrorAction Stop`
          : `printf 'denied' > '${denied.replaceAll("'", "'\\''")}'`;
      const blocked = await runtime.ctx.shell.execute(
        runtime.ctx.shell.resolve({ command: outsideCommand, workdir: root }),
      );
      const blockedResult = await blocked.result();
      assert.notEqual(blockedResult.exitCode, 0);
      await assert.rejects(readFile(denied), { code: "ENOENT" });
    } finally {
      await runtime.ctx.fiber.dispose();
      await rm(root, { recursive: true, force: true });
      await rm(outside, { recursive: true, force: true });
    }
  },
);
