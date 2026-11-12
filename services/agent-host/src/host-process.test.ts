import assert from "node:assert/strict";
import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process";
import { mkdtemp, mkdir, readdir, rm } from "node:fs/promises";
import { createServer } from "node:http";
import { createInterface } from "node:readline";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { randomUUID } from "node:crypto";
import { test } from "node:test";

/** 独立客户端通过真实管道驱动宿主，不直接调用宿主内部方法。 */
class Client {
  readonly child: ChildProcessWithoutNullStreams;
  private next = 0;
  private pending = new Map<
    number,
    { resolve: (value: unknown) => void; reject: (error: Error) => void }
  >();
  readonly events: unknown[] = [];
  private diagnostics = "";
  /** 测试密钥只存在于此客户端与临时 HTTP 服务的内存中。 */
  constructor(home: string) {
    const bundle = process.env.GITMASTER_AI_TEST_BUNDLE_ROOT;
    const program = bundle
      ? join(bundle, process.platform === "win32" ? "node.exe" : "node")
      : process.execPath;
    const entry = bundle ? join(bundle, "dist/main.js") : "dist/main.js";
    const hooks = bundle
      ? [
          "--import",
          new URL("../fixtures/bundle-isolation.ts", import.meta.url).href,
        ]
      : [];
    this.child = spawn(program, [...hooks, entry, "--home", home], {
      cwd: bundle ?? new URL("..", import.meta.url),
      stdio: "pipe",
      windowsHide: true,
    });
    // 此子进程只处理临时假模型；保留有界启动诊断帮助定位打包遗漏。
    this.child.stderr.on("data", (chunk: Buffer) => {
      this.diagnostics = (this.diagnostics + chunk.toString("utf8")).slice(
        -4000,
      );
    });
    createInterface({ input: this.child.stdout }).on("line", (line) => {
      const value: unknown = JSON.parse(line);
      if (!value || typeof value !== "object") return;
      const frame = value as Record<string, unknown>;
      if (frame.method === "gitmaster/credential")
        this.child.stdin.write(
          `${JSON.stringify({ jsonrpc: "2.0", id: frame.id, result: "temporary-process-key" })}\n`,
        );
      else if (typeof frame.id === "number") {
        const item = this.pending.get(frame.id);
        if (!item) return;
        this.pending.delete(frame.id);
        if (frame.error) item.reject(new Error(JSON.stringify(frame.error)));
        else item.resolve(frame.result);
      } else if (frame.method === "gitmaster/event")
        this.events.push(frame.params);
    });
    this.child.on("exit", () => {
      for (const item of this.pending.values())
        item.reject(new Error(`宿主提前退出\n${this.diagnostics}`));
      this.pending.clear();
    });
  }
  /** 每次调用都拥有独立 ID 与短超时，超时不会自动重发写任务。 */
  call<T>(method: string, params: unknown): Promise<T> {
    return new Promise<T>((resolve, reject) => {
      const id = ++this.next;
      const timer = setTimeout(
        () => {
          this.pending.delete(id);
          reject(
            new Error(
              `请求超时：${method}，exitCode=${this.child.exitCode}\n${this.diagnostics}`,
            ),
          );
        },
        method === "gitmaster/configure" ? 90000 : 15000,
      );
      this.pending.set(id, {
        resolve: (value) => {
          clearTimeout(timer);
          resolve(value as T);
        },
        reject: (error) => {
          clearTimeout(timer);
          reject(error);
        },
      });
      this.child.stdin.write(
        `${JSON.stringify({ jsonrpc: "2.0", id, method, params })}\n`,
      );
    });
  }
  /** 正常关闭后等待进程退出，不把发送关闭命令等同于已经回收。 */
  async close(): Promise<void> {
    if (this.child.exitCode !== null) return;
    const exit = new Promise<void>((resolve) =>
      this.child.once("exit", () => resolve()),
    );
    await this.call("gitmaster/shutdown", {});
    await exit;
  }
}

test(
  "独立宿主保存会话、恢复历史且不自动重发模型请求",
  { timeout: 180000 },
  async () => {
    const root = await mkdtemp(join(tmpdir(), "gitmaster-host-process-"));
    const project = join(root, "project");
    const home = join(root, "home");
    await mkdir(project);
    let requests = 0;
    const server = createServer(async (request, response) => {
      for await (const _chunk of request) {
        /* 消费受控测试请求，避免连接积压。 */
      }
      requests++;
      response.writeHead(200, { "content-type": "text/event-stream" });
      response.end(
        `data: ${JSON.stringify({ id: "chatcmpl-host", object: "chat.completion.chunk", created: 0, model: "fixture", choices: [{ index: 0, delta: { role: "assistant", content: "宿主回复已保存" }, finish_reason: null }] })}\n\ndata: ${JSON.stringify({ id: "chatcmpl-host", object: "chat.completion.chunk", created: 0, model: "fixture", choices: [{ index: 0, delta: {}, finish_reason: "stop" }] })}\n\ndata: [DONE]\n\n`,
      );
    });
    await new Promise<void>((resolve) =>
      server.listen(0, "127.0.0.1", resolve),
    );
    const address = server.address();
    assert.ok(address && typeof address !== "string");
    const providers = [
      {
        providerId: "process-fixture",
        displayName: "进程测试",
        protocol: "openai-completions",
        baseUrl: `http://127.0.0.1:${address.port}/v1`,
        credentialRef: "GITMASTER_AI_TEST",
        allowInsecureHttp: false,
        models: [
          {
            modelId: "fixture",
            displayName: "模型",
            contextWindow: 8192,
            maxOutputTokens: 1024,
          },
        ],
      },
    ];
    let client: Client | undefined;
    try {
      client = new Client(home);
      await client.call("gitmaster/configure", { providers });
      const session = await client.call<{ sessionId: string }>("create", {
        workspaceRoot: project,
        repositoryKey: "fixture-repository",
        model: {
          providerId: "process-fixture",
          modelId: "fixture",
          reasoningEffort: null,
        },
        permission: "read-only",
      });
      const inputId = randomUUID();
      await client.call("send", {
        sessionId: session.sessionId,
        clientMessageId: inputId,
        text: "保存这段历史",
      });
      const deadline = Date.now() + 15000;
      while (
        (await client.call<{ busy: boolean }>("activity", {})).busy &&
        Date.now() < deadline
      )
        await new Promise<void>((resolve) => setTimeout(resolve, 30));
      const history = await client.call<{ events: unknown[] }>("read", {
        sessionId: session.sessionId,
        after: 0,
      });
      assert.match(JSON.stringify(history.events), /宿主回复已保存/);
      await client.call("send", {
        sessionId: session.sessionId,
        clientMessageId: inputId,
        text: "保存这段历史",
      });
      assert.equal(requests, 1);
      await client.close();
      client = new Client(home);
      await client.call("gitmaster/configure", { providers: [] });
      const restored = await client.call<{ events: unknown[] }>("read", {
        sessionId: session.sessionId,
        after: 0,
      });
      assert.match(JSON.stringify(restored.events), /保存这段历史/);
      assert.match(JSON.stringify(restored.events), /宿主回复已保存/);
      assert.equal(requests, 1);
      const files = await readdir(join(home, "sessions"), { recursive: true });
      assert.ok(files.length > 0);
      await assert.rejects(
        client.call("read", { sessionId: "../../escape", after: 0 }),
      );
      await client.call("delete", {
        sessionId: session.sessionId,
        confirmed: true,
      });
      await assert.rejects(
        client.call("read", { sessionId: session.sessionId, after: 0 }),
      );
      const remaining = await readdir(join(home, "sessions"), {
        recursive: true,
      });
      assert.ok(
        !remaining.some((path) => path.includes(session.sessionId)),
        "删除会话必须同时移除受控目录内的历史",
      );
      await client.call("gitmaster/configure", { providers });
      const blank = await client.call<{ sessionId: string }>("create", {
        workspaceRoot: project,
        repositoryKey: "fixture-repository",
        model: {
          providerId: "process-fixture",
          modelId: "fixture",
          reasoningEffort: null,
        },
        permission: "read-only",
      });
      await client.close();
      client = new Client(home);
      await client.call("gitmaster/configure", { providers: [] });
      const blankPage = await client.call<{ status: string }>("read", {
        sessionId: blank.sessionId,
        after: 0,
      });
      assert.equal(
        blankPage.status,
        "idle",
        "空白会话也必须能恢复，且不能自行请求模型",
      );
      assert.equal(requests, 1);
      await client.call("delete", {
        sessionId: blank.sessionId,
        confirmed: true,
      });
      await client.close();
      client = undefined;
    } finally {
      if (client) {
        await client.close().catch(() => client?.child.kill());
      }
      server.closeAllConnections();
      await new Promise<void>((resolve) => server.close(() => resolve()));
      await rm(root, { recursive: true, force: true });
    }
  },
);
