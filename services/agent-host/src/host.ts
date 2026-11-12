import { randomUUID } from "node:crypto";
import {
  lstat,
  mkdir,
  readFile,
  realpath,
  rename,
  rm,
  writeFile,
} from "node:fs/promises";
import { basename, dirname, isAbsolute, join, relative, sep } from "node:path";
import { z } from "zod";
import {
  MessageId,
  ReasoningEffortId,
  type ContentBlock,
} from "@deepseek-ai/dsh-llm";
import { SessionId, type SessionEvent } from "@deepseek-ai/dsh-session";
import type { AgentHandle } from "@deepseek-ai/dsh-agent";
import type {} from "@deepseek-ai/dsh-session-persistence";
import type {
  AiEvent,
  AiEventEnvelope,
  AiModelRef,
  AiPermission,
  AiProvider,
  AiSession,
  AiJobSnapshot,
} from "./contracts.ts";
import { createRuntime } from "./runtime.ts";
import { providerProfiles, validateProvider } from "./models.ts";
import { describeModelError } from "./errors.ts";
import { SandboxPreflight, type SandboxStatus } from "./sandbox-preflight.ts";
import { registerGitTools } from "./git-tools.ts";

const idSchema = z.string().uuid();
const modelSchema = z.strictObject({
  providerId: z.string().min(1).max(64),
  modelId: z.string().min(1).max(200),
  reasoningEffort: z.string().max(64).nullable(),
});
const permissionSchema = z.enum([
  "read-only",
  "workspace-write",
  "danger-full-access",
]);
type Runtime = Awaited<ReturnType<typeof createRuntime>>;
type Decision = {
  sessionId: string;
  accept: (value: unknown) => void;
  cancel: () => void;
};
/** 写租约只由 Rust 侧签发，取消时撤回尚未获得的租约。 */
export interface MutationBridge {
  acquire: (sessionId: string, signal: AbortSignal) => Promise<void>;
  release: (sessionId: string) => void;
  sandboxStatus?: (sessionId: string) => Promise<SandboxStatus>;
  handoff?: (sessionId: string) => Promise<void>;
  git?: (
    sessionId: string,
    phase: "read" | "prepare" | "execute",
    request: Record<string, unknown>,
    signal?: AbortSignal,
  ) => Promise<unknown>;
}

/** 对外只返回稳定错误码，不传播可能包含密钥的提供商异常原文。 */
export function safeCode(error: unknown): string {
  if (
    error &&
    typeof error === "object" &&
    "code" in error &&
    typeof error.code === "string" &&
    /^[A-Z_]{2,64}$/.test(error.code)
  )
    return error.code;
  return error instanceof z.ZodError ? "AI_INVALID_INPUT" : "AI_RUNTIME_FAILED";
}
/** 对展示副本设置字节预算，完整结果仍由上游会话/输出机制持有。 */
function bounded(text: string, limit = 16000): string {
  const bytes = Buffer.from(text);
  return bytes.length <= limit
    ? text
    : `${bytes.subarray(0, limit).toString("utf8")}\n[显示已截断]`;
}
/** 仅提取文本块，不将请求头、隐藏元数据或凭据送到 UI。 */
function textOf(blocks: readonly ContentBlock[]): string {
  return bounded(
    blocks
      .filter((block) => block.type === "text")
      .map((block) => block.text)
      .join("\n"),
  );
}
/** 创建稳定业务错误，调用方只会看到 code。 */
function fail(code: string): never {
  throw Object.assign(new Error(code), { code });
}

/** 桌面独立会话宿主；业务循环与工具实现均由固定上游组合承担。 */
export class AgentHost {
  private readonly runtime: Runtime;
  private readonly home: string;
  private readonly emit: (event: AiEventEnvelope) => void;
  private readonly epoch = randomUUID();
  private readonly sessions = new Map<string, AiSession>();
  private readonly handles = new Map<string, Promise<AgentHandle>>();
  private readonly materialized = new Set<string>();
  private readonly events = new Map<string, AiEventEnvelope[]>();
  private readonly jobHistory = new Map<
    string,
    AiJobSnapshot & { sessionId: string }
  >();
  private readonly decisions = new Map<string, Decision>();
  private readonly runs = new Map<string, string>();
  private readonly compactions = new Map<string, AbortController>();
  private indexWrites: Promise<void> = Promise.resolve();
  private readonly streams = new Map<
    string,
    {
      messageId: string;
      text: string;
      reasoning: string;
      timer?: ReturnType<typeof setTimeout>;
    }
  >();
  private providers: AiProvider[] = [];
  private active: string | null = null;
  private readonly mutations: MutationBridge | undefined;
  private readonly leased = new Set<string>();
  private leaseTimer: ReturnType<typeof setInterval> | undefined;
  private sequence = 0;
  private readonly preflight: SandboxPreflight | undefined;
  private readonly positions = new Map<
    string,
    { turn: number | null; step: number | null; time: number }
  >();
  /** 捕获受信任目录与通知回调，不能由模型修改宿主路径。 */
  constructor(
    runtime: Runtime,
    home: string,
    emit: (event: AiEventEnvelope) => void,
    mutations?: MutationBridge,
  ) {
    this.runtime = runtime;
    this.home = home;
    this.emit = emit;
    this.mutations = mutations;
    this.preflight = mutations?.sandboxStatus
      ? new SandboxPreflight(mutations.sandboxStatus)
      : undefined;
    if (mutations?.git)
      registerGitTools(runtime.ctx, (sessionId, request, write, signal) =>
        this.executeGit(sessionId, request, write, signal),
      );
    runtime.ctx.on("agent/error", ({ agent, error }) =>
      this.publish(agent.id, {
        kind: "error",
        ...describeModelError(error),
      }),
    );
    runtime.ctx.on("tools/execute", async (exec, next) => {
      if (["write", "edit", "pwsh", "bash"].includes(exec.name)) {
        if (!exec.agent || !this.mutations) fail("AI_MUTATION_UNAVAILABLE");
        const sessionId = exec.agent.id;
        if (["pwsh", "bash"].includes(exec.name)) {
          await this.preflight?.check(
            sessionId,
            this.sessions.get(sessionId)?.permission ?? "read-only",
          );
          const handles = await Promise.all(this.handles.values());
          const count = handles
            .flatMap((handle) => runtime.ctx.jobs.list(handle.agent.id))
            .filter(
              (job) => job.status === "running" || job.status === "stopping",
            ).length;
          if (count >= 8) fail("AI_JOB_LIMIT");
        }
        if (!this.leased.has(sessionId)) {
          await this.mutations.acquire(sessionId, exec.signal);
          if (exec.signal.aborted) {
            this.mutations.release(sessionId);
            fail("AI_CANCELLED");
          }
          this.leased.add(sessionId);
        }
        this.leaseTimer ??= setInterval(() => {
          void this.releaseIdleLeases();
        }, 250);
      }
      const result = await next();
      if (exec.agent && ["pwsh", "bash"].includes(exec.name) && result.isError)
        this.preflight?.rememberFailure(exec.agent.id, textOf(result.content));
      return result;
    });
    runtime.ctx.on("session/event", (session, event) => {
      const projected = this.project(session.id, event);
      if (projected) this.publish(session.id, projected);
    });
    runtime.ctx.on("agent/assistant-stream", ({ agent, frame }) => {
      if (frame.type === "start")
        this.streams.set(agent.id, {
          messageId: `${frame.turn}-${frame.step}`,
          text: "",
          reasoning: "",
        });
      if (
        frame.type !== "chunk" ||
        (frame.chunk.type !== "text-delta" &&
          frame.chunk.type !== "reasoning-delta")
      )
        return;
      const stream = this.streams.get(agent.id);
      if (!stream) return;
      if (frame.chunk.type === "text-delta")
        stream.text = bounded(stream.text + frame.chunk.text);
      else stream.reasoning = bounded(stream.reasoning + frame.chunk.text);
      if (!stream.timer)
        stream.timer = setTimeout(() => {
          stream.timer = undefined;
          this.publish(
            agent.id,
            {
              kind: "assistant",
              messageId: stream.messageId,
              text: bounded(stream.text),
              reasoning: bounded(stream.reasoning),
              final: false,
            },
            false,
          );
        }, 34);
    });
    runtime.ctx.on("approval/request", async (request) => {
      const sessionId = request.agent.id;
      const approvalId = randomUUID();
      this.publish(sessionId, { kind: "status", status: "awaitingApproval" });
      const detail = request.callId
        ? request.agent.session
            .snapshotEvents()
            .findLast(
              (event) =>
                event.type === "tool/call" &&
                event.data.callId === request.callId,
            )
        : undefined;
      this.publish(sessionId, {
        kind: "approval",
        approvalId,
        title: request.toolName,
        detail: bounded(
          `${request.reason ?? "此操作需要扩大权限"}\n${detail?.type === "tool/call" ? detail.data.arguments : ""}`,
        ),
        withdrawn: false,
      });
      try {
        return await this.waitDecision(
          approvalId,
          sessionId,
          request.signal,
          z.enum(["allowed-once", "rejected"]),
        );
      } finally {
        this.publish(sessionId, {
          kind: "approval",
          approvalId,
          title: request.toolName,
          detail: "",
          withdrawn: true,
        });
        this.publish(sessionId, {
          kind: "status",
          status: request.signal?.aborted ? "stopping" : "running",
        });
      }
    });
    runtime.ctx.on("user-questions/request", async (request) => {
      if (!request.agent) return fail("AI_QUESTION_OWNER");
      const sessionId = request.agent.id;
      const questionId = randomUUID();
      this.publish(sessionId, { kind: "status", status: "awaitingAnswer" });
      this.publish(sessionId, {
        kind: "question",
        questionId,
        title: request.questions
          .map((question) => question.question)
          .join("\n"),
        detail: JSON.stringify(request.questions),
        withdrawn: false,
      });
      const schema = z.strictObject({
        answers: z.array(
          z.strictObject({
            id: z.string(),
            selected: z.array(z.string()),
            custom: z.string().optional(),
          }),
        ),
      });
      try {
        return await this.waitDecision(
          questionId,
          sessionId,
          request.signal,
          schema,
        );
      } finally {
        this.publish(sessionId, {
          kind: "question",
          questionId,
          title: "",
          detail: "",
          withdrawn: true,
        });
        this.publish(sessionId, {
          kind: "status",
          status: request.signal?.aborted ? "stopping" : "running",
        });
      }
    });
  }
  /** 只加载应用自有索引，损坏配置不能静默替换成空历史。 */
  async initialize(): Promise<void> {
    await mkdir(this.home, { recursive: true });
    let value: string;
    try {
      value = await readFile(join(this.home, "session-index.json"), "utf8");
    } catch (error) {
      if (
        error &&
        typeof error === "object" &&
        "code" in error &&
        error.code === "ENOENT"
      )
        return;
      throw error;
    }
    const schema = z
      .array(
        z.strictObject({
          sessionId: idSchema,
          workspaceRoot: z.string(),
          repositoryKey: z.string(),
          title: z.string().max(200),
          model: modelSchema,
          permission: permissionSchema,
          createdAt: z.string(),
          updatedAt: z.string(),
        }),
      )
      .max(1000);
    for (const session of schema.parse(JSON.parse(value))) {
      this.sessions.set(session.sessionId, session);
      this.materialized.add(session.sessionId);
    }
  }
  /** 配置刷新只更新后续请求的 profile，已准备的调用继续使用其快照。 */
  async configure(providers: unknown): Promise<void> {
    const parsed = z
      .array(z.unknown())
      .max(64)
      .parse(providers)
      .map(validateProvider);
    if (
      new Set(parsed.map((provider) => provider.providerId)).size !==
      parsed.length
    )
      fail("AI_INVALID_PROVIDER");
    await this.runtime.modelFiber.update({
      providers: providerProfiles(parsed),
    });
    this.providers = parsed;
  }
  /** 将会话索引原子发布，文件中不包含 API 密钥。 */
  private saveIndex(): Promise<void> {
    const task = this.indexWrites.then(async () => {
      const temporary = join(this.home, `.${randomUUID()}.tmp`);
      await writeFile(temporary, JSON.stringify([...this.sessions.values()]), {
        flag: "wx",
        mode: 0o600,
      });
      try {
        await rename(temporary, join(this.home, "session-index.json"));
      } catch (error) {
        await rm(temporary, { force: true });
        throw error;
      }
    });
    this.indexWrites = task.catch(() => {
      /* 本次失败由调用者处理，后续保存仍可重试。 */
    });
    return task;
  }
  /** 校验模型的确切提供商与 ID，不静默回退到其他模型。 */
  private model(model: AiModelRef): void {
    if (
      !this.providers.some(
        (provider) =>
          provider.providerId === model.providerId &&
          provider.models.some((entry) => entry.modelId === model.modelId),
      )
    )
      fail("AI_UNKNOWN_MODEL");
  }
  /** 会话只能通过宿主索引定位，ID 不能拼接为任意路径。 */
  private session(id: string): AiSession {
    return this.sessions.get(id) ?? fail("AI_SESSION_NOT_FOUND");
  }
  /** 创建或恢复精确会话，并复用同一份并发初始化 Promise。 */
  private handle(session: AiSession): Promise<AgentHandle> {
    const existing = this.handles.get(session.sessionId);
    if (existing) return existing;
    const options = {
      provider: session.model.providerId,
      model: session.model.modelId,
      ...(session.model.reasoningEffort
        ? { reasoningEffort: ReasoningEffortId(session.model.reasoningEffort) }
        : {}),
    };
    const promise = (
      this.materialized.has(session.sessionId)
        ? this.runtime.ctx.agents.resume({
            resumeSessionId: SessionId(session.sessionId),
            agentOptions: options,
          })
        : this.runtime.ctx.agents.create({
            sessionId: SessionId(session.sessionId),
            meta: { cwd: session.workspaceRoot },
            agentOptions: options,
          })
    )
      .then((handle) => {
        this.materialized.add(session.sessionId);
        this.runtime.ctx.permissionPresets.set(
          handle.agent.session,
          session.permission,
        );
        if (!this.events.has(session.sessionId)) {
          this.events.set(session.sessionId, []);
          for (const event of handle.agent.session.snapshotEvents()) {
            const projected = this.project(session.sessionId, event);
            if (projected) this.publish(session.sessionId, projected);
          }
        }
        return handle;
      })
      .catch((error: unknown) => {
        this.handles.delete(session.sessionId);
        throw error;
      });
    this.handles.set(session.sessionId, promise);
    return promise;
  }
  /** 从上游持久事件提取 UI 需要的最小副本。 */
  private project(sessionId: string, event: SessionEvent): AiEvent | null {
    const previousPosition = this.positions.get(sessionId);
    const data = event.data as { turn?: number; step?: number };
    this.positions.set(sessionId, {
      turn: data.turn ?? previousPosition?.turn ?? null,
      step:
        data.step ??
        (event.type === "turn/start" ? null : (previousPosition?.step ?? null)),
      time: event.time,
    });
    if (event.type === "turn/end" && event.data.reason.kind === "error")
      this.publish(sessionId, {
        kind: "error",
        ...describeModelError(event.data.reason.error),
      });
    if (event.type === "turn/start")
      return { kind: "turn", turn: event.data.turn, state: "running" };
    if (event.type === "turn/end")
      return {
        kind: "turn",
        turn: event.data.turn,
        state:
          event.data.reason.kind === "error"
            ? "failed"
            : event.data.reason.kind === "aborted"
              ? "cancelled"
              : event.data.reason.kind === "interrupted"
                ? "interrupted"
                : event.data.reason.kind === "completed"
                  ? "completed"
                  : "interrupted",
      };
    if (event.type === "job/registered" || event.type === "job/settled") {
      const callId = event.data.toolCall?.rootCallId ?? null;
      const call = this.events
        .get(sessionId)
        ?.findLast(
          (entry) =>
            entry.event.kind === "tool" && entry.event.toolCallId === callId,
        )?.event;
      let args: { command?: string; workdir?: string } = {};
      try {
        args = z
          .object({
            command: z.string().optional(),
            workdir: z.string().optional(),
          })
          .parse(JSON.parse(call?.kind === "tool" ? call.input : "{}"));
      } catch {
        /* 非法参数只影响摘要，不能改变实际执行。 */
      }
      const previous = this.jobHistory.get(event.data.jobId);
      this.jobHistory.set(event.data.jobId, {
        jobId: event.data.jobId,
        sessionId,
        label: event.data.label,
        command: args.command ?? event.data.label,
        workingDirectory:
          args.workdir ?? this.sessions.get(sessionId)?.workspaceRoot ?? "",
        output: previous?.output ?? "",
        state: event.type === "job/settled" ? event.data.status : "running",
        detail:
          event.type === "job/settled" ? (event.data.detail ?? null) : null,
        toolCallId: callId,
      });
      const retained = [...this.jobHistory.values()].filter(
        (job) => job.sessionId === sessionId,
      );
      for (const old of retained.slice(0, Math.max(0, retained.length - 100)))
        this.jobHistory.delete(old.jobId);
    }
    if (event.type === "user/message" && event.data.source.kind === "user")
      return { kind: "user", text: textOf(event.data.content) };
    if (event.type === "assistant/message") {
      const stream = this.streams.get(sessionId);
      if (stream?.timer) clearTimeout(stream.timer);
      return {
        kind: "assistant",
        messageId: `${event.data.turn}-${event.data.step}`,
        text: textOf(event.data.message.content),
        reasoning: bounded(
          event.data.message.content
            .filter((block) => block.type === "reasoning")
            .map((block) => block.text)
            .join("\n"),
        ),
        final: true,
      };
    }
    if (event.type === "tool/call")
      return {
        kind: "tool",
        toolCallId: event.data.callId,
        name: event.data.name,
        input: bounded(event.data.arguments),
        output: null,
        failed: false,
      };
    if (event.type === "tool/result") {
      for (const job of this.jobHistory.values())
        if (
          job.sessionId === sessionId &&
          job.toolCallId === event.data.message.toolCallId
        )
          job.output = textOf(event.data.message.content);
      const previous = this.events
        .get(sessionId)
        ?.findLast(
          (entry) =>
            entry.event.kind === "tool" &&
            entry.event.toolCallId === event.data.message.toolCallId,
        )?.event;
      return {
        kind: "tool",
        toolCallId: event.data.message.toolCallId,
        name: previous?.kind === "tool" ? previous.name : "工具",
        input: "",
        output: textOf(event.data.message.content),
        failed: event.data.message.isError === true,
      };
    }
    return null;
  }
  /** 实时增量不进入投影历史；最终消息以独立序号保证可恢复。 */
  private publish(sessionId: string, event: AiEvent, retain = true): void {
    const previous = this.events.get(sessionId)?.at(-1)?.event;
    if (
      event.kind === "error" &&
      previous?.kind === "error" &&
      event.code === previous.code &&
      event.message === previous.message
    )
      return;
    const envelope: AiEventEnvelope = {
      runtimeEpoch: this.epoch,
      sessionId,
      sequence: ++this.sequence,
      runId: this.runs.get(sessionId) ?? null,
      ...this.positions.get(sessionId),
      event,
    };
    if (retain) {
      const events = this.events.get(sessionId) ?? [];
      events.push(envelope);
      if (events.length > 5000) events.splice(0, events.length - 5000);
      this.events.set(sessionId, events);
    }
    this.emit(envelope);
  }
  /** 回答绑定原始请求闭包和取消信号，迟到响应不能授权操作。 */
  private async executeGit(
    sessionId: string,
    request: Record<string, unknown>,
    write: boolean,
    signal: AbortSignal,
  ): Promise<unknown> {
    const session = this.sessions.get(sessionId);
    if (!session || !this.mutations?.git || !this.mutations.handoff)
      fail("AI_GIT_UNAVAILABLE");
    if (write && session.permission === "read-only") fail("AI_GIT_READ_ONLY");
    for (const [id, promise] of this.handles) {
      if (this.sessions.get(id)?.repositoryKey !== session.repositoryKey)
        continue;
      const handle = await promise;
      if (
        this.runtime.ctx.jobs
          .list(handle.agent.id)
          .some((job) => job.status === "running" || job.status === "stopping")
      )
        fail("AI_GIT_BUSY");
    }
    // 先等原生确认释放外部租约，再进入同仓库 Git 队列，后续文件/Shell 写入按需重新获取。
    await this.mutations.handoff(sessionId);
    this.leased.delete(sessionId);
    if (signal.aborted) fail("AI_CANCELLED");
    if (!write) return this.mutations.git(sessionId, "read", request, signal);
    const preview = z
      .object({ planId: z.string() })
      .passthrough()
      .parse(await this.mutations.git(sessionId, "prepare", request));
    const approvalId = randomUUID();
    const title = `Git ${String(request.action)}`;
    const detail = JSON.stringify(preview, null, 2);
    if (Buffer.byteLength(detail) > 96000) fail("AI_GIT_OUTPUT_LIMIT");
    this.publish(sessionId, { kind: "status", status: "awaitingApproval" });
    this.publish(sessionId, {
      kind: "approval",
      approvalId,
      title,
      detail,
      withdrawn: false,
    });
    try {
      const decision = await this.waitDecision(
        approvalId,
        sessionId,
        signal,
        z.enum(["allowed-once", "rejected"]),
      );
      if (decision !== "allowed-once" || signal.aborted) fail("AI_CANCELLED");
      return await this.mutations.git(sessionId, "execute", {
        planId: preview.planId,
      });
    } finally {
      this.publish(sessionId, {
        kind: "approval",
        approvalId,
        title,
        detail: "",
        withdrawn: true,
      });
      this.publish(sessionId, {
        kind: "status",
        status: signal.aborted ? "stopping" : "running",
      });
    }
  }
  /** 回答绑定原始请求闭包和取消信号，迟到响应不能授权操作。 */
  private waitDecision<T>(
    id: string,
    sessionId: string,
    signal: AbortSignal | undefined,
    schema: z.ZodType<T>,
  ): Promise<T> {
    return new Promise<T>((resolve, reject) => {
      /** 一次结算后立即撤销应答入口和信号监听。 */
      const finish = (value?: T): void => {
        clearTimeout(timer);
        this.decisions.delete(id);
        signal?.removeEventListener("abort", cancel);
        if (value === undefined)
          reject(
            Object.assign(new Error("AI_CANCELLED"), { code: "AI_CANCELLED" }),
          );
        else resolve(value);
      };
      /** 取消只撤回当前问题，不接受后来的按钮响应。 */
      const cancel = (): void => finish();
      const timer = setTimeout(cancel, 300000);
      this.decisions.set(id, {
        sessionId,
        accept: (value) => finish(schema.parse(value)),
        cancel,
      });
      signal?.addEventListener("abort", cancel, { once: true });
      if (signal?.aborted) cancel();
    });
  }
  /** 后台进程仍运行时保留写租约，前台等待超时不代表写入结束。 */
  private async releaseIdleLeases(): Promise<void> {
    for (const sessionId of this.leased) {
      if (this.runs.has(sessionId)) continue;
      const handle = await this.handles.get(sessionId);
      if (
        handle &&
        this.runtime.ctx.jobs
          .list(handle.agent.id)
          .some((job) => job.status === "running" || job.status === "stopping")
      )
        continue;
      this.mutations?.release(sessionId);
      this.leased.delete(sessionId);
    }
    if (!this.leased.size && this.leaseTimer) {
      clearInterval(this.leaseTimer);
      this.leaseTimer = undefined;
    }
  }
  /** 使用上游提供的存储位置，仅删除应用历史根目录下对应 UUID 的会话目录。 */
  private async removeHistory(sessionId: string): Promise<void> {
    const root = await realpath(join(this.home, "sessions"));
    const snapshots = await this.runtime.ctx.sessionPersistence.list();
    for (const snapshot of snapshots) {
      if (
        snapshot.header.id !== sessionId ||
        !snapshot.location ||
        snapshot.location.kind !== "jsonl"
      )
        continue;
      const directory = dirname(snapshot.location.path);
      const path = relative(root, await realpath(directory));
      if (
        isAbsolute(path) ||
        path.startsWith(`..${sep}`) ||
        path === ".." ||
        path.split(sep).length !== 2 ||
        basename(directory) !== sessionId ||
        (await lstat(directory)).isSymbolicLink()
      )
        fail("AI_HISTORY_PATH");
      await rm(directory, { recursive: true });
    }
  }
  /** 分发固定方法；这里不是向 WebView 开放的通用执行接口。 */
  async dispatch(method: string, params: unknown): Promise<unknown> {
    if (method === "invalidateSandbox") {
      this.preflight?.invalidate();
      return null;
    }
    if (method === "activity") {
      const handles = await Promise.all(this.handles.values());
      return {
        busy:
          this.active !== null ||
          this.leased.size > 0 ||
          handles.some((handle) =>
            this.runtime.ctx.jobs
              .list(handle.agent.id)
              .some(
                (job) => job.status === "running" || job.status === "stopping",
              ),
          ),
      };
    }
    if (method === "testModel") {
      const selection = z
        .strictObject({
          providerId: z.string().min(1).max(64),
          modelId: z.string().min(1).max(200),
        })
        .parse(params);
      this.model({ ...selection, reasoningEffort: null });
      if (this.active) fail("AI_BUSY");
      this.active = "connection-test";
      try {
        let received = false;
        for await (const chunk of this.runtime.ctx.llm.stream({
          provider: selection.providerId,
          model: selection.modelId,
          messages: [
            { role: "user", content: [{ type: "text", text: "Reply OK." }] },
          ],
          maxTokens: 16,
          signal: AbortSignal.timeout(30000),
        })) {
          if (
            chunk.type === "finish" &&
            (chunk.reason.kind === "error" || chunk.reason.kind === "aborted")
          )
            return {
              ok: false,
              error: describeModelError(chunk.reason.failure),
            };
          if (
            ((chunk.type === "text-delta" ||
              chunk.type === "reasoning-delta") &&
              chunk.text) ||
            (chunk.type === "block-end" &&
              (chunk.block.type === "text" ||
                chunk.block.type === "reasoning") &&
              chunk.block.text)
          )
            received = true;
        }
        if (!received) fail("AI_CONNECTION_FAILED");
        return { ok: true };
      } catch (error) {
        return { ok: false, error: describeModelError(error) };
      } finally {
        if (this.active === "connection-test") this.active = null;
      }
    }
    if (method === "list") {
      const input = z
        .strictObject({
          repositoryKey: z.string(),
          workspaceRoot: z.string().min(1),
          offset: z.number().int().min(0).default(0),
        })
        .parse(params);
      const workspaceRoot = await realpath(input.workspaceRoot);
      const sessions = [...this.sessions.values()]
        .filter(
          (session) =>
            session.repositoryKey === input.repositoryKey &&
            session.workspaceRoot === workspaceRoot,
        )
        .sort((a, b) => b.updatedAt.localeCompare(a.updatedAt));
      return {
        items: sessions.slice(input.offset, input.offset + 50),
        nextOffset:
          input.offset + 50 < sessions.length ? input.offset + 50 : null,
      };
    }
    if (method === "create") {
      const input = z
        .strictObject({
          workspaceRoot: z.string().min(1),
          repositoryKey: z.string().min(1),
          model: modelSchema,
          permission: permissionSchema,
        })
        .parse(params);
      this.model(input.model);
      const now = new Date().toISOString();
      const session: AiSession = {
        ...input,
        workspaceRoot: await realpath(input.workspaceRoot),
        sessionId: randomUUID(),
        title: "新会话",
        createdAt: now,
        updatedAt: now,
      };
      if (this.sessions.size >= 1000) fail("AI_SESSION_LIMIT");
      this.sessions.set(session.sessionId, session);
      try {
        await this.handle(session);
        await this.saveIndex();
        return session;
      } catch (error) {
        this.sessions.delete(session.sessionId);
        throw error;
      }
    }
    const input = z.object({ sessionId: idSchema }).passthrough().parse(params);
    const session = this.session(input.sessionId);
    if (method === "read") {
      const cursor = z
        .object({ after: z.number().int().min(0).default(0) })
        .parse(params).after;
      await this.handle(session);
      const events: AiEventEnvelope[] = [];
      let bytes = 0;
      for (const event of this.events.get(session.sessionId) ?? []) {
        if (event.sequence <= cursor) continue;
        const size = Buffer.byteLength(JSON.stringify(event));
        if (bytes + size > 96000 && events.length) break;
        bytes += size;
        events.push(event);
      }
      const latestStatus = this.events
        .get(session.sessionId)
        ?.findLast((entry) => entry.event.kind === "status")?.event;
      return {
        session,
        events,
        status: this.runs.has(session.sessionId)
          ? latestStatus?.kind === "status"
            ? latestStatus.status
            : "running"
          : "idle",
        nextSequence: events.at(-1)?.sequence ?? cursor,
        more:
          (this.events.get(session.sessionId)?.at(-1)?.sequence ?? 0) >
          (events.at(-1)?.sequence ?? cursor),
      };
    }
    if (method === "send") {
      const message = z
        .object({
          clientMessageId: idSchema,
          text: z.string().trim().min(1).max(16000),
        })
        .parse(params);
      this.model(session.model);
      const handle = await this.handle(session);
      if (
        handle.agent.session
          .snapshotEvents()
          .some(
            (event) =>
              event.type === "user/message" &&
              event.data.id === message.clientMessageId,
          )
      )
        return { runId: message.clientMessageId };
      if (this.active || handle.agent.status !== "idle") fail("AI_BUSY");
      this.active = session.sessionId;
      this.runs.set(session.sessionId, message.clientMessageId);
      session.updatedAt = new Date().toISOString();
      if (session.title === "新会话") session.title = message.text.slice(0, 40);
      try {
        await this.saveIndex();
      } catch (error) {
        this.runs.delete(session.sessionId);
        this.active = null;
        throw error;
      }
      this.publish(session.sessionId, { kind: "status", status: "running" });
      handle.agent.followup({
        role: "user",
        id: MessageId(message.clientMessageId),
        source: { kind: "user" },
        content: [{ type: "text", text: message.text }],
      });
      void handle.agent
        .whenIdle()
        .catch((error: unknown) =>
          this.publish(session.sessionId, {
            kind: "error",
            ...describeModelError(error),
          }),
        )
        .finally(() => {
          this.runs.delete(session.sessionId);
          if (this.active === session.sessionId) this.active = null;
          this.publish(session.sessionId, { kind: "status", status: "idle" });
        });
      return { runId: message.clientMessageId };
    }
    if (method === "cancel") {
      const runId = z.object({ runId: idSchema }).parse(params).runId;
      if (this.runs.get(session.sessionId) !== runId) fail("AI_STALE_RUN");
      this.compactions.get(session.sessionId)?.abort();
      (await this.handle(session)).agent.cancel({ kind: "user" });
      this.publish(session.sessionId, { kind: "status", status: "stopping" });
      return null;
    }
    if (method === "update") {
      if (this.runs.has(session.sessionId)) fail("AI_BUSY");
      const patch = z
        .object({
          title: z.string().trim().min(1).max(200).optional(),
          model: modelSchema.optional(),
          permission: permissionSchema.optional(),
        })
        .parse(params);
      const handle = await this.handle(session);
      if (patch.permission) {
        if (
          this.runtime.ctx.jobs
            .list(handle.agent.id)
            .some(
              (job) => job.status === "running" || job.status === "stopping",
            )
        )
          fail("AI_JOBS_RUNNING");
        this.runtime.ctx.permissionPresets.set(
          handle.agent.session,
          patch.permission,
        );
        session.permission = patch.permission;
        this.preflight?.invalidate();
      }
      if (patch.model) {
        this.model(patch.model);
        handle.agent.model.select({
          provider: patch.model.providerId,
          model: patch.model.modelId,
          ...(patch.model.reasoningEffort
            ? {
                reasoningEffort: ReasoningEffortId(patch.model.reasoningEffort),
              }
            : {}),
        });
        session.model = patch.model;
      }
      if (patch.title) session.title = patch.title;
      await this.saveIndex();
      return session;
    }
    if (method === "decision") {
      const value = z
        .object({ decisionId: idSchema, value: z.unknown() })
        .parse(params);
      const decision = this.decisions.get(value.decisionId);
      if (!decision || decision.sessionId !== session.sessionId)
        fail("AI_DECISION_EXPIRED");
      decision.accept(value.value);
      return null;
    }
    if (method === "jobs") {
      const handle = await this.handle(session);
      const live = this.runtime.ctx.jobs
        .list(handle.agent.id)
        .filter((job) => job.owner === handle.agent.id);
      const snapshots = [...this.jobHistory.values()]
        .filter((job) => job.sessionId === session.sessionId)
        .map(({ sessionId: _sessionId, ...job }) => ({
          ...job,
          state:
            (job.state === "running" || job.state === "stopping") &&
            !live.some((entry) => entry.id === job.jobId)
              ? "interrupted"
              : job.state,
        }));
      for (const job of live) {
        const output = this.runtime.ctx.jobs.readAt(job.id, 0, handle.agent.id);
        const value: AiJobSnapshot = {
          jobId: job.id,
          label: job.label,
          state: job.status,
          output: bounded(
            `${output.lossy ? "[较早输出已截断]\n" : ""}${output.chunks.map((chunk) => `${chunk.channel === "stderr" ? "[stderr] " : ""}${chunk.text}`).join("")}`,
          ),
          command: this.jobHistory.get(job.id)?.command ?? job.label,
          workingDirectory:
            this.jobHistory.get(job.id)?.workingDirectory ??
            session.workspaceRoot,
          detail: job.detail ?? null,
          toolCallId: this.jobHistory.get(job.id)?.toolCallId ?? null,
        };
        const index = snapshots.findIndex((entry) => entry.jobId === job.id);
        if (index >= 0) snapshots[index] = value;
        else snapshots.push(value);
      }
      return snapshots.slice(-20).map((job) => ({
        ...job,
        output: bounded(job.output, 4096),
        command: bounded(job.command, 1024),
        workingDirectory: bounded(job.workingDirectory, 1024),
        label: bounded(job.label, 256),
        detail: job.detail ? bounded(job.detail, 512) : null,
      }));
    }
    if (method === "stopJob") {
      const { jobId } = z
        .object({ jobId: z.string().min(1).max(200) })
        .parse(params);
      const handle = await this.handle(session);
      const job = this.runtime.ctx.jobs
        .list(handle.agent.id)
        .find((job) => job.id === jobId);
      if (!job) fail("AI_JOB_NOT_FOUND");
      this.runtime.ctx.jobs.kill(job.id, handle.agent.id, "用户请求停止");
      return null;
    }
    if (method === "delete") {
      const confirmed = z
        .object({ confirmed: z.boolean() })
        .parse(params).confirmed;
      const handle = await this.handle(session);
      const jobs = this.runtime.ctx.jobs.list(handle.agent.id);
      if (
        (this.runs.has(session.sessionId) ||
          jobs.some(
            (job) => job.status === "running" || job.status === "stopping",
          )) &&
        !confirmed
      )
        fail("AI_CONFIRM_CLOSE");
      handle.agent.cancel({ kind: "user" });
      for (const job of jobs)
        if (job.status === "running" || job.status === "stopping")
          this.runtime.ctx.jobs.kill(job.id, handle.agent.id);
      await handle.dispose();
      this.handles.delete(session.sessionId);
      await this.removeHistory(session.sessionId);
      this.sessions.delete(session.sessionId);
      this.events.delete(session.sessionId);
      this.materialized.delete(session.sessionId);
      for (const job of this.jobHistory.values())
        if (job.sessionId === session.sessionId)
          this.jobHistory.delete(job.jobId);
      await this.saveIndex();
      return null;
    }
    if (method === "compact") {
      if (this.active) fail("AI_BUSY");
      const handle = await this.handle(session);
      if (this.active) fail("AI_BUSY");
      const controller = new AbortController();
      const runId = randomUUID();
      this.active = session.sessionId;
      this.runs.set(session.sessionId, runId);
      this.compactions.set(session.sessionId, controller);
      this.publish(session.sessionId, {
        kind: "status",
        status: "running",
        reason: "正在压缩上下文",
      });
      void this.runtime.ctx.compaction
        .compactNow(handle.agent, controller.signal)
        .catch((error: unknown) =>
          this.publish(session.sessionId, {
            kind: "error",
            ...describeModelError(error),
          }),
        )
        .finally(() => {
          this.compactions.delete(session.sessionId);
          this.runs.delete(session.sessionId);
          if (this.active === session.sessionId) this.active = null;
          this.publish(session.sessionId, { kind: "status", status: "idle" });
        });
      return { runId };
    }
    fail("AI_UNKNOWN_METHOD");
  }
  /** 退出时先取消所有交互，再等待 Agent 与原生子进程释放。 */
  async close(): Promise<void> {
    for (const controller of this.compactions.values()) controller.abort();
    if (this.leaseTimer) clearInterval(this.leaseTimer);
    for (const stream of this.streams.values())
      if (stream.timer) clearTimeout(stream.timer);
    for (const decision of this.decisions.values()) decision.cancel();
    await Promise.allSettled(
      [...this.handles.values()].map(async (promise) => {
        const handle = await promise;
        handle.agent.cancel({ kind: "user" });
        for (const job of this.runtime.ctx.jobs.list(handle.agent.id))
          if (job.status === "running" || job.status === "stopping")
            this.runtime.ctx.jobs.kill(job.id, handle.agent.id);
        await handle.dispose();
      }),
    );
    await this.runtime.ctx.fiber.dispose();
    for (const id of this.leased) this.mutations?.release(id);
    this.leased.clear();
  }
}
