import { randomUUID } from "node:crypto";
import { resolve } from "node:path";
import { createRuntime } from "./runtime.ts";
import { AgentHost, safeCode } from "./host.ts";
import { encodeFrame, FrameDecoder, type RpcFrame } from "./protocol.ts";
import { z } from "zod";
import type { SandboxStatus } from "./sandbox-preflight.ts";

const homeIndex = process.argv.indexOf("--home");
if (homeIndex < 0 || !process.argv[homeIndex + 1])
  throw new Error("AI_HOME_REQUIRED");
const home = resolve(process.argv[homeIndex + 1]!);
const pending = new Map<
  string,
  { resolve: (value: unknown) => void; reject: (reason: Error) => void }
>();
/** stdout 是私有协议，任何诊断文字都不得写入这里。 */
function send(frame: unknown): void {
  process.stdout.write(encodeFrame(frame));
}
/** 运行时向 Rust 请求受控能力；模型不能访问该传输对象。 */
function request(
  method: string,
  params: unknown,
  signal?: AbortSignal,
): Promise<unknown> {
  return new Promise((accept, reject) => {
    const id = `host-${randomUUID()}`;
    /** 请求只结算一次；断开与取消移除其应答入口。 */
    const finish = (value: unknown, code?: string): void => {
      clearTimeout(timer);
      signal?.removeEventListener("abort", cancel);
      pending.delete(id);
      if (code) reject(Object.assign(new Error(code), { code }));
      else accept(value);
    };
    /** 撤回未执行请求，迟到的批准响应不会重新触发模型工具。 */
    const cancel = (): void => {
      if (method === "gitmaster/git" && signal?.aborted) {
        send({
          jsonrpc: "2.0",
          method: "gitmaster/cancel-git",
          params: { requestId: id },
        });
        return;
      }
      finish(null, signal?.aborted ? "AI_CANCELLED" : "AI_BRIDGE_UNAVAILABLE");
    };
    const timer = setTimeout(
      () => finish(null, "AI_TIMEOUT"),
      method === "gitmaster/git" ? 600000 : 125000,
    );
    pending.set(id, {
      resolve: (value) => finish(value),
      reject: (error) => finish(null, safeCode(error)),
    });
    signal?.addEventListener("abort", cancel, { once: true });
    if (signal?.aborted) finish(null, "AI_CANCELLED");
    else send({ jsonrpc: "2.0", id, method, params });
  });
}
const runtime = await createRuntime({
  home,
  credentials: {
    resolve: async (ref) => {
      const value = await request("gitmaster/credential", { ref });
      return z.string().max(16384).nullable().parse(value) ?? undefined;
    },
  },
});
const host = new AgentHost(
  runtime,
  home,
  (event) => send({ jsonrpc: "2.0", method: "gitmaster/event", params: event }),
  {
    sandboxStatus: async (sessionId) =>
      (await request("gitmaster/sandbox-status", {
        sessionId,
      })) as SandboxStatus,
    handoff: async (sessionId) => {
      await request("gitmaster/handoff", { sessionId });
    },
    git: (sessionId, phase, gitRequest, signal) =>
      request(
        "gitmaster/git",
        { sessionId, phase, request: gitRequest },
        signal,
      ),
    acquire: async (sessionId, signal) => {
      try {
        await request("gitmaster/acquire-write", { sessionId }, signal);
      } catch (error) {
        send({
          jsonrpc: "2.0",
          method: "gitmaster/release-write",
          params: { sessionId },
        });
        throw error;
      }
    },
    release: (sessionId) =>
      send({
        jsonrpc: "2.0",
        method: "gitmaster/release-write",
        params: { sessionId },
      }),
  },
);
await host.initialize();
let closing = false;
/** 固定入口方法经过宿主校验，错误响应不包含提供商异常原文。 */
async function dispatch(frame: RpcFrame): Promise<void> {
  if (!("method" in frame)) {
    const entry =
      typeof frame.id === "string" ? pending.get(frame.id) : undefined;
    if (frame.error) {
      const code = /^[A-Z_]{2,64}$/.test(frame.error.message)
        ? frame.error.message
        : "AI_BRIDGE_FAILED";
      entry?.reject(Object.assign(new Error(code), { code }));
    } else entry?.resolve(frame.result);
    return;
  }
  try {
    let result: unknown;
    if (frame.method === "gitmaster/configure") {
      const config = z
        .strictObject({ providers: z.array(z.unknown()).max(64) })
        .parse(frame.params);
      await host.configure(config.providers);
      result = null;
    } else if (frame.method === "gitmaster/shutdown") {
      closing = true;
      await host.close();
      result = null;
    } else result = await host.dispatch(frame.method, frame.params);
    send({ jsonrpc: "2.0", id: frame.id, result });
    if (closing) {
      process.stdin.pause();
      process.stdout.write("", () => process.exit(0));
    }
  } catch (error) {
    send({
      jsonrpc: "2.0",
      id: frame.id,
      error: { code: -32000, message: safeCode(error) },
    });
  }
}
const decoder = new FrameDecoder();
process.stdin.on("data", (chunk: Buffer) => {
  try {
    for (const frame of decoder.feed(chunk)) void dispatch(frame);
  } catch {
    process.stdin.destroy();
  }
});
/** 父进程断开必须回收工具任务，不依赖窗口或组件卸载。 */
async function shutdown(): Promise<void> {
  if (closing) return;
  closing = true;
  for (const entry of pending.values())
    entry.reject(new Error("AI_DISCONNECTED"));
  await host.close();
  process.exit(0);
}
process.stdin.on("end", () => {
  void shutdown();
});
process.stdin.on("error", () => {
  void shutdown();
});
process.on("SIGTERM", () => {
  void shutdown();
});
process.on("SIGINT", () => {
  void shutdown();
});
send({
  jsonrpc: "2.0",
  method: "gitmaster/ready",
  params: { version: 1, platform: process.platform },
});
