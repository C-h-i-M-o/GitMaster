import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";
import { mkdtemp, mkdir, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { test } from "node:test";
import {
  LlmAdapter,
  ToolCallId,
  type GenerateOptions,
  type StreamChunk,
} from "@deepseek-ai/dsh-llm";
import { AgentHost, type MutationBridge } from "./host.ts";
import { SessionId } from "@deepseek-ai/dsh-session";
import { createRuntime } from "./runtime.ts";
import type {
  AiEventEnvelope,
  AiJobSnapshot,
  AiProvider,
  AiSession,
  AiSessionPage,
} from "./contracts.ts";

/** 受控模型只提出预定工具请求，实际执行、审批和后台回收由上游完成。 */
class ToolFixture extends LlmAdapter {
  readonly name: string;
  readonly args: Record<string, unknown>;
  /** 每轮先调用一次工具，然后根据真实工具结果结束。 */
  constructor(name: string, args: Record<string, unknown>) {
    super();
    this.name = name;
    this.args = args;
  }
  /** 不伪造工具结果或权限事件。 */
  async *stream(options: GenerateOptions): AsyncIterable<StreamChunk> {
    if (options.messages.at(-1)?.role === "user") {
      yield {
        type: "block-end",
        index: 0,
        block: {
          type: "tool-call",
          id: ToolCallId(randomUUID()),
          name: this.name,
          arguments: this.args,
        },
      };
      yield { type: "finish", reason: "tool-calls" };
    } else {
      yield {
        type: "block-end",
        index: 0,
        block: { type: "text", text: "工具处理已结束" },
      };
      yield { type: "finish", reason: "stop" };
    }
  }
}

/** 有界轮询只用于等待真实异步状态，不以固定睡眠当作成功证据。 */
async function until<T>(
  read: () => T | Promise<T>,
  predicate: (value: T) => boolean,
): Promise<T> {
  const deadline = Date.now() + 20000;
  while (Date.now() < deadline) {
    const value = await read();
    if (predicate(value)) return value;
    await new Promise<void>((resolve) => setTimeout(resolve, 30));
  }
  throw new Error("等待真实状态超时");
}

/** 创建独立目录与只含假模型的宿主，桥接仅记录租约生命周期。 */
async function fixture(
  name: string,
  args: Record<string, unknown>,
  adapter?: LlmAdapter,
  bridge: Partial<MutationBridge> = {},
) {
  const root = await mkdtemp(join(tmpdir(), "gitmaster-host-contract-"));
  const project = join(root, "project");
  await mkdir(project);
  const runtime = await createRuntime({
    home: join(root, "home"),
    workspaceRoot: project,
  });
  const events: AiEventEnvelope[] = [];
  const leased = new Set<string>();
  const host = new AgentHost(
    runtime,
    join(root, "home"),
    (event) => events.push(event),
    {
      acquire: async (id) => {
        leased.add(id);
      },
      release: (id) => {
        leased.delete(id);
      },
      ...bridge,
    },
  );
  await host.initialize();
  const provider: AiProvider = {
    providerId: "fixture",
    displayName: "测试",
    protocol: "openai-completions",
    baseUrl: "https://example.invalid/v1",
    credentialRef: null,
    allowInsecureHttp: false,
    models: [
      {
        modelId: "fixture",
        displayName: "模型",
        contextWindow: 8192,
        maxOutputTokens: 1024,
      },
    ],
  };
  await host.configure([provider]);
  await runtime.modelFiber.dispose();
  runtime.ctx.llm.registerAdapter(
    ["fixture"],
    adapter ?? new ToolFixture(name, args),
  );
  /** 关闭后才清理测试自己的目录和进程资源。 */
  async function close(): Promise<void> {
    await host.close();
    await rm(root, { recursive: true, force: true });
  }
  /** 新会话精确绑定同一个临时项目与测试模型。 */
  async function create(
    permission: "read-only" | "danger-full-access",
  ): Promise<AiSession> {
    return (await host.dispatch("create", {
      workspaceRoot: project,
      repositoryKey: "fixture",
      model: {
        providerId: "fixture",
        modelId: "fixture",
        reasoningEffort: null,
      },
      permission,
    })) as AiSession;
  }
  return {
    host,
    runtime,
    events,
    leased,
    project,
    root,
    provider,
    close,
    create,
  };
}

/** 复现服务商通过终态返回错误或只输出推理内容，不触发任何真实模型请求。 */
class TerminalFixture extends LlmAdapter {
  private readonly chunks: readonly StreamChunk[];
  /** 固定每次响应，保持测试可重复。 */
  constructor(chunks: readonly StreamChunk[]) {
    super();
    this.chunks = chunks;
  }
  /** 使用真实上游流消费逻辑检查终态，不能把部分文本误当成功。 */
  async *stream(): AsyncIterable<StreamChunk> {
    yield* this.chunks;
  }
}

test("连接测试保留流内证书错误，部分文本不能覆盖失败终态", async () => {
  const f = await fixture(
    "read",
    {},
    new TerminalFixture([
      { type: "text-delta", index: 0, text: "部分回复" },
      {
        type: "finish",
        reason: {
          kind: "error",
          failure: {
            code: "TRANSPORT",
            message:
              "Connection error.: self-signed certificate in certificate chain (SELF_SIGNED_CERT_IN_CHAIN) Bearer sk-test-private",
          },
        },
      },
    ]),
  );
  try {
    const result = (await f.host.dispatch("testModel", {
      providerId: "fixture",
      modelId: "fixture",
    })) as { ok: boolean; error?: { code: string; message: string } };
    assert.equal(result.ok, false);
    assert.equal(result.error?.code, "AI_TLS_CERTIFICATE");
    assert.match(result.error?.message ?? "", /SELF_SIGNED_CERT_IN_CHAIN/);
    assert.doesNotMatch(JSON.stringify(result), /sk-test-private|Bearer/);
  } finally {
    await f.close();
  }
});

test("连接测试接受输出预算内的推理响应", async () => {
  const f = await fixture(
    "read",
    {},
    new TerminalFixture([
      { type: "reasoning-delta", index: 0, text: "思考响应" },
      { type: "finish", reason: { kind: "max-tokens" } },
    ]),
  );
  try {
    assert.deepEqual(
      await f.host.dispatch("testModel", {
        providerId: "fixture",
        modelId: "fixture",
      }),
      { ok: true },
    );
  } finally {
    await f.close();
  }
});

test("会话实时与重启历史均显示证书根因且不暴露凭据", async () => {
  const f = await fixture(
    "read",
    {},
    new TerminalFixture([
      {
        type: "finish",
        reason: {
          kind: "error",
          failure: {
            code: "TRANSPORT",
            message:
              "Connection error.: SELF_SIGNED_CERT_IN_CHAIN Bearer sk-test-private",
          },
        },
      },
    ]),
  );
  let closed = false;
  let restored: AgentHost | undefined;
  try {
    const session = await f.create("read-only");
    await f.host.dispatch("send", {
      sessionId: session.sessionId,
      clientMessageId: randomUUID(),
      text: "测试错误",
    });
    const event = await until(
      () => f.events.find((e) => e.event.kind === "error"),
      Boolean,
    );
    assert.equal(event?.event.kind, "error");
    if (event?.event.kind === "error")
      assert.match(event.event.message, /SELF_SIGNED_CERT_IN_CHAIN/);
    await until(
      () => f.host.dispatch("activity", {}) as Promise<{ busy: boolean }>,
      (v) => !v.busy,
    );
    await f.host.close();
    closed = true;
    const runtime = await createRuntime({
      home: join(f.root, "home"),
      workspaceRoot: f.project,
    });
    restored = new AgentHost(runtime, join(f.root, "home"), () => {});
    await restored.initialize();
    await restored.configure([f.provider]);
    const history = (await restored.dispatch("read", {
      sessionId: session.sessionId,
    })) as AiSessionPage;
    assert.equal(typeof history.runtimeEpoch, "string");
    assert.equal(history.activeRunId, null);
    const error = history.events.find((e) => e.event.kind === "error")?.event;
    assert.equal(error?.kind, "error");
    if (error?.kind === "error")
      assert.match(error.message, /SELF_SIGNED_CERT_IN_CHAIN/);
    assert.doesNotMatch(
      JSON.stringify(history.events),
      /sk-test-private|Bearer/,
    );
  } finally {
    await restored?.close();
    if (!closed) await f.close();
    else await rm(f.root, { recursive: true, force: true });
  }
});

test(
  "只读写入需审批，拒绝和取消后的迟到批准均不能写文件",
  { timeout: 60000 },
  async () => {
    const f = await fixture("write", {
      file_path: "result.txt",
      content: "经批准的内容",
      sandbox_permissions: "workspace-write",
      justification: "需要写入本次临时测试文件",
    });
    try {
      const session = await f.create("read-only");
      const id = randomUUID();
      await f.host.dispatch("send", {
        sessionId: session.sessionId,
        clientMessageId: id,
        text: "写入测试文件",
      });
      const approval = await until(
        () =>
          f.events.find(
            (entry) =>
              entry.event.kind === "approval" && !entry.event.withdrawn,
          ),
        Boolean,
      ).catch(() => {
        throw new Error(JSON.stringify(f.events));
      });
      assert.ok(approval?.event.kind === "approval");
      const pendingPage = (await f.host.dispatch("read", {
        sessionId: session.sessionId,
        after: Number.MAX_SAFE_INTEGER,
      })) as AiSessionPage;
      assert.equal(pendingPage.activeRunId, id);
      assert.equal(pendingPage.events.length, 0);
      assert.equal(typeof pendingPage.runtimeEpoch, "string");
      await f.host.dispatch("decision", {
        sessionId: session.sessionId,
        decisionId: approval.event.approvalId,
        value: "rejected",
      });
      await until(
        () => f.host.dispatch("activity", {}) as Promise<{ busy: boolean }>,
        (value) => !value.busy,
      );
      await assert.rejects(readFile(join(f.project, "result.txt")), {
        code: "ENOENT",
      });
      const offset = f.events.length;
      const next = randomUUID();
      await f.host.dispatch("send", {
        sessionId: session.sessionId,
        clientMessageId: next,
        text: "再次写入测试文件",
      });
      const pending = await until(
        () =>
          f.events
            .slice(offset)
            .find(
              (entry) =>
                entry.event.kind === "approval" && !entry.event.withdrawn,
            ),
        Boolean,
      );
      assert.ok(pending?.event.kind === "approval");
      await f.host.dispatch("cancel", {
        sessionId: session.sessionId,
        runId: next,
      });
      await until(
        () => f.host.dispatch("activity", {}) as Promise<{ busy: boolean }>,
        (value) => !value.busy,
      );
      await assert.rejects(
        f.host.dispatch("decision", {
          sessionId: session.sessionId,
          decisionId: pending.event.approvalId,
          value: "allowed-once",
        }),
      );
      await assert.rejects(readFile(join(f.project, "result.txt")), {
        code: "ENOENT",
      });
      assert.equal(f.leased.size, 0);
    } finally {
      await f.close();
    }
  },
);

test(
  "后台任务隔离会话且保持写租约，界面读取不消耗模型输出",
  { timeout: 60000 },
  async () => {
    const shell = process.platform === "win32" ? "pwsh" : "bash";
    const command =
      process.platform === "win32"
        ? "Write-Output 'background-output'; Start-Sleep -Seconds 30"
        : "printf 'background-output'; sleep 30";
    const f = await fixture(shell, {
      command,
      description: "后台测试",
      run_in_background: true,
    });
    try {
      const session = await f.create("danger-full-access");
      const other = await f.create("danger-full-access");
      await f.host.dispatch("send", {
        sessionId: session.sessionId,
        clientMessageId: randomUUID(),
        text: "运行临时后台命令",
      });
      const jobs = await until(
        () =>
          f.host.dispatch("jobs", { sessionId: session.sessionId }) as Promise<
            AiJobSnapshot[]
          >,
        (value) =>
          value.some((job) => job.output.includes("background-output")),
      );
      assert.equal(f.leased.has(session.sessionId), true);
      assert.deepEqual(
        await f.host.dispatch("jobs", { sessionId: other.sessionId }),
        [],
      );
      await assert.rejects(
        f.host.dispatch("stopJob", {
          sessionId: other.sessionId,
          jobId: jobs[0]!.jobId,
        }),
      );
      const again = (await f.host.dispatch("jobs", {
        sessionId: session.sessionId,
      })) as AiJobSnapshot[];
      assert.match(again[0]!.output, /background-output/);
      const modelJob = f.runtime.ctx.jobs
        .list(SessionId(session.sessionId))
        .find((job) => job.id === jobs[0]!.jobId);
      assert.ok(modelJob);
      const modelOutput = f.runtime.ctx.jobs.read(
        modelJob.id,
        SessionId(session.sessionId),
      );
      assert.match(
        modelOutput.chunks.map((chunk) => chunk.text).join(""),
        /background-output/,
        "UI 观察不能消耗模型尚未领取的输出",
      );
      await f.host.dispatch("stopJob", {
        sessionId: session.sessionId,
        jobId: jobs[0]!.jobId,
      });

      test(
        "用户问题暂停轮次并将多选及自定义回答回填模型工具结果",
        { timeout: 60000 },
        async () => {
          const f = await fixture("ask_user_question", {
            questions: [
              {
                id: "choice",
                question: "请选择测试值",
                options: [{ label: "选项甲" }, { label: "选项乙" }],
                multiSelect: true,
              },
            ],
          });
          try {
            const session = await f.create("read-only");
            await f.host.dispatch("send", {
              sessionId: session.sessionId,
              clientMessageId: randomUUID(),
              text: "向用户提问",
            });
            const pending = await until(
              () =>
                f.events.find(
                  (entry) =>
                    entry.event.kind === "question" && !entry.event.withdrawn,
                ),
              Boolean,
            );
            assert.ok(pending?.event.kind === "question");
            await f.host.dispatch("decision", {
              sessionId: session.sessionId,
              decisionId: pending.event.questionId,
              value: {
                answers: [
                  { id: "choice", selected: ["选项甲"], custom: "附加说明" },
                ],
              },
            });
            await until(
              () =>
                f.host.dispatch("activity", {}) as Promise<{ busy: boolean }>,
              (value) => !value.busy,
            );
            const output = f.events
              .filter(
                (entry) =>
                  entry.event.kind === "tool" && entry.event.output !== null,
              )
              .map((entry) =>
                entry.event.kind === "tool" ? entry.event.output : "",
              )
              .join("\n");
            assert.match(output, /选项甲/);
            assert.match(output, /附加说明/);
          } finally {
            await f.close();
          }
        },
      );
      const ended = await until(
        () =>
          f.host.dispatch("jobs", { sessionId: session.sessionId }) as Promise<
            AiJobSnapshot[]
          >,
        (value) => value[0]?.state === "killed",
      );
      assert.equal(ended[0]?.state, "killed");
      await until(
        () => f.host.dispatch("activity", {}) as Promise<{ busy: boolean }>,
        (value) => !value.busy,
      );
      assert.equal(f.leased.size, 0);
    } finally {
      await f.close();
    }
  },
);

/** 即使完全访问，专用 Git 写工具也必须在预览之后逐次等待用户确认。 */
test("专用 Git 写入确认与只读拒绝、租约交接", { timeout: 60000 }, async () => {
  const phases: string[] = [];
  const f = await fixture("git_stage", { paths: ["明确文件.txt"] }, undefined, {
    handoff: async () => {
      phases.push("handoff");
    },
    git: async (_id, phase, request) => {
      phases.push(phase);
      if (phase === "prepare") {
        assert.deepEqual(request, { action: "stage", paths: ["明确文件.txt"] });
        return { planId: "fixture-plan", paths: ["明确文件.txt"] };
      }
      return { result: { outcome: "succeeded" } };
    },
  });
  try {
    const session = await f.create("danger-full-access");
    await f.host.dispatch("send", {
      sessionId: session.sessionId,
      clientMessageId: randomUUID(),
      text: "暂存明确文件",
    });
    const approval = await until(
      () =>
        f.events.find(
          (entry) => entry.event.kind === "approval" && !entry.event.withdrawn,
        ),
      Boolean,
    );
    assert.ok(approval?.event.kind === "approval");
    assert.deepEqual(phases, ["handoff", "prepare"]);
    await f.host.dispatch("decision", {
      sessionId: session.sessionId,
      decisionId: approval.event.approvalId,
      value: "allowed-once",
    });
    await until(
      () => f.host.dispatch("activity", {}) as Promise<{ busy: boolean }>,
      (value) => !value.busy,
    );
    assert.deepEqual(phases, ["handoff", "prepare", "execute"]);
    const readOnly = await f.create("read-only");
    const offset = phases.length;
    await f.host.dispatch("send", {
      sessionId: readOnly.sessionId,
      clientMessageId: randomUUID(),
      text: "只读模式拒绝暂存",
    });
    await until(
      () => f.host.dispatch("activity", {}) as Promise<{ busy: boolean }>,
      (value) => !value.busy,
    );
    assert.equal(phases.length, offset);
    assert.match(JSON.stringify(f.events), /AI_GIT_READ_ONLY/);
  } finally {
    await f.close();
  }
});

test("共享 Git 公共目录的不同工作树分别列出各自会话", async () => {
  const f = await fixture("read", { file_path: "unused.txt" });
  try {
    const first = await f.create("read-only");
    const sibling = join(dirname(f.project), "another-worktree");
    await mkdir(sibling);
    await f.host.dispatch("create", {
      workspaceRoot: sibling,
      repositoryKey: "fixture",
      model: first.model,
      permission: "read-only",
    });
    const page = (await f.host.dispatch("list", {
      repositoryKey: "fixture",
      workspaceRoot: f.project,
      offset: 0,
    })) as { items: AiSession[] };
    assert.deepEqual(
      page.items.map((session) => session.sessionId),
      [first.sessionId],
    );
  } finally {
    await f.close();
  }
});
