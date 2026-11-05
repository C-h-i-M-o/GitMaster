import assert from "node:assert/strict";
import test, { after } from "node:test";
import { build } from "vite";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { rm } from "node:fs/promises";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import {
  chatMenuDisabledReason,
  permissionPresentation,
  previewLongText,
  projectAgentTranscript,
  providerThinking,
} from "../src/ui/agentPresentation.ts";
import type {
  AgentEvent,
  EventPayload,
  ProviderProfile,
} from "../src/types/agent.ts";

/** 创建指定聊天和运行的持久事件夹具。 */
function event(
  sequence: number,
  runId: string,
  payload: EventPayload,
): AgentEvent {
  return {
    version: 1,
    sequence,
    sessionId: "chat-test",
    runId,
    timestamp: sequence,
    ...payload,
  };
}

const profile: ProviderProfile = {
  id: "profile-test",
  name: "Fixture",
  protocol: "chatCompletions",
  baseUrl: "https://provider.example/v1",
  model: "fixture-model",
  credentialId: "credential-test",
};

let agentTurnComponent: typeof import("../src/components/AgentTurn").AgentTurn | null = null;
const ssrOutputDirectory = resolve(
  "node_modules/.vite-temp/agent-transcript-ssr",
);

/** 通过 Vite SSR 转换真实 TSX 组件以验证实际渲染结果。 */
async function renderAgentTurn(
  turn: ReturnType<typeof projectAgentTranscript>[number],
) {
  if (!agentTurnComponent) {
    await build({
    configFile: false,
      build: {
        ssr: resolve("src/components/AgentTurn.tsx"),
        outDir: ssrOutputDirectory,
        emptyOutDir: true,
        minify: false,
        rollupOptions: {
          output: { entryFileNames: "agent-turn.mjs", format: "es" },
        },
      },
    });
    const bundle = await import(
      pathToFileURL(resolve(ssrOutputDirectory, "agent-turn.mjs")).href
    );
    agentTurnComponent = bundle.AgentTurn;
  }
  return renderToStaticMarkup(
    createElement(agentTurnComponent, { turn, activeRun: null }),
  );
}

test("按 runId/callId 合并工具调用结果并去重重复、乱序事件", () => {
  const call = {
    id: "call-1",
    name: "read_file",
    arguments: { path: "src/app.ts" },
  };
  const started = event(1, "run-1", {
    kind: "runStarted",
    profile,
    permissionMode: "requestApproval",
  });
  const assistant = event(3, "run-1", {
    kind: "assistantMessage",
    text: "我会先读取文件。",
    protocol: "chatCompletions",
    wire: {
      reasoning_content: "供应商明确返回的思考摘要",
      content: "我会先读取文件。",
    },
    calls: [call],
  });
  const turnList = projectAgentTranscript([
    event(8, "run-1", {
      kind: "runEnded",
      status: "failed",
      message: "执行失败",
    }),
    event(6, "run-1", {
      kind: "approvalDecided",
      approvalId: "approval-1",
      approved: true,
    }),
    event(5, "run-1", {
      kind: "approvalRequested",
      approval: {
        id: "approval-1",
        call,
        reason: "需要确认",
        target: "src/app.ts",
        preview: null,
      },
    }),
    event(4, "run-1", { kind: "toolCall", call }),
    assistant,
    assistant,
    event(2, "run-1", { kind: "userMessage", text: "检查文件" }),
    started,
    event(7, "run-1", {
      kind: "toolResult",
      callId: "call-1",
      name: "read_file",
      result: { message: "读取失败" },
      isError: true,
    }),
  ]);

  assert.equal(turnList.length, 1);
  assert.equal(turnList[0]?.runId, "run-1");
  assert.equal(turnList[0]?.userMessage, "检查文件");
  assert.equal(turnList[0]?.assistantMessages.length, 1);
  assert.equal(turnList[0]?.tools.length, 1);
  assert.equal(turnList[0]?.tools[0]?.callId, "call-1");
  assert.equal(turnList[0]?.tools[0]?.approved, true);
  assert.equal(turnList[0]?.tools[0]?.isError, true);
  assert.equal(turnList[0]?.status, "failed");
  assert.equal(turnList[0]?.hasError, true);
});

test("只投影供应商显式提供的可展示思考摘要", () => {
  assert.deepEqual(
    providerThinking("chatCompletions", {
      reasoning_content: "可展示内容",
      content: "回答正文",
    }),
    ["可展示内容"],
  );
  assert.deepEqual(
    providerThinking("responses", {
      output: [
        {
          type: "reasoning",
          summary: [{ type: "summary_text", text: "明确摘要" }],
          content: [{ type: "reasoning_text", text: "不透明内容" }],
          encrypted_content: "opaque",
        },
      ],
    }),
    ["明确摘要"],
  );
  assert.deepEqual(
    providerThinking("anthropicMessages", {
      content: [
        { type: "thinking", thinking: "供应商明确返回的思考文本", signature: "signed" },
        { type: "redacted_thinking", data: "opaque" },
      ],
    }),
    ["供应商明确返回的思考文本"],
  );
});

test("真实 SSR 中审批与错误可见、工具结果同卡、最终回复可读", async () => {
  const call = {
    id: "call-render",
    name: "write_file",
    arguments: { path: "result.txt" },
  };
  const [turn] = projectAgentTranscript([
    event(1, "run-render", {
      kind: "runStarted",
      profile,
      permissionMode: "requestApproval",
    }),
    event(2, "run-render", {
      kind: "approvalRequested",
      approval: {
        id: "approval-render",
        call,
        reason: "写入项目文件需要确认",
        target: "result.txt",
        preview: null,
      },
    }),
    event(3, "run-render", {
      kind: "toolResult",
      callId: call.id,
      name: call.name,
      result: { written: true },
      isError: true,
    }),
    event(4, "run-render", {
      kind: "assistantMessage",
      text: "处理已结束。",
      protocol: "chatCompletions",
      wire: { content: "处理已结束。" },
      calls: [],
    }),
    event(5, "run-render", {
      kind: "runEnded",
      status: "failed",
      message: "工具执行失败",
    }),
  ]);
  assert.ok(turn);
  const markup = await renderAgentTurn(turn);
  assert.match(markup, /本轮有操作失败/);
  assert.match(markup, /有操作等待审批/);
  assert.match(markup, /class="agent-turn-process" open=""/);
  assert.match(markup, /class="agent-tool-card is-error" open=""/);
  const toolCard = markup.match(
    /<details class="agent-tool-card is-error" open="">([\s\S]*?)<\/details>/,
  )?.[1];
  assert.ok(toolCard);
  assert.match(toolCard, /调用参数[\s\S]*失败详情/);
  assert.match(markup, /处理已结束。/);
});

test("真实 SSR 中已完成的工具卡默认折叠且展示配对结果", async () => {
  const call = {
    id: "call-done",
    name: "read_file",
    arguments: { path: "README.md" },
  };
  const [turn] = projectAgentTranscript([
    event(1, "run-done", {
      kind: "runStarted",
      profile,
      permissionMode: "autoApprove",
    }),
    event(2, "run-done", { kind: "userMessage", text: "读取 README" }),
    event(3, "run-done", { kind: "toolCall", call }),
    event(4, "run-done", {
      kind: "toolResult",
      callId: call.id,
      name: call.name,
      result: { text: "内容已读取" },
      isError: false,
    }),
    event(5, "run-done", {
      kind: "assistantMessage",
      text: "文件内容已读取。",
      protocol: "chatCompletions",
      wire: { content: "文件内容已读取。" },
      calls: [],
    }),
    event(6, "run-done", {
      kind: "runEnded",
      status: "completed",
      message: null,
    }),
  ]);
  assert.ok(turn);
  const markup = await renderAgentTurn(turn);
  const toolCard = markup.match(
    /<details class="agent-tool-card ">([\s\S]*?)<\/details>/,
  )?.[1];
  assert.ok(toolCard);
  assert.match(toolCard, /内容已读取/);
  assert.match(markup, /class="agent-turn-process"/);
  assert.doesNotMatch(markup, /class="agent-turn-process" open=""/);
  assert.match(markup, /文件内容已读取。/);
  assert.match(markup, /data-agent-sequence="2"/);
  assert.match(markup, /data-agent-sequence="5"/);
});

after(async () => {
  await rm(ssrOutputDirectory, { recursive: true, force: true });
  agentTurnComponent = null;
});

test("待审批保持醒目，工具或运行失败标为 attention", () => {
  const call = {
    id: "call-pending",
    name: "write_file",
    arguments: { path: "out.txt" },
  };
  const [turn] = projectAgentTranscript([
    event(1, "run-pending", {
      kind: "runStarted",
      profile,
      permissionMode: "autoApprove",
    }),
    event(2, "run-pending", {
      kind: "approvalRequested",
      approval: {
        id: "approval-pending",
        call,
        reason: "需要用户批准",
        target: "out.txt",
        preview: null,
      },
    }),
  ]);
  assert.equal(turn?.hasPendingApproval, true);
  assert.equal(turn?.hasError, false);
  assert.equal(turn?.status, "running");
});

test("权限说明复用已定义描述，长文本提供预览和完整内容", () => {
  const permission = permissionPresentation("fullAccess", [
    {
      value: "fullAccess",
      label: "完全访问权限",
      description: "注册工具可访问任意文件和网络。",
    },
  ]);
  assert.equal(permission.label, "完全访问权限");
  assert.match(permission.description, /任意文件和网络/);
  assert.ok(permission.icon);

  const longText = "很长的正文。".repeat(40);
  const preview = previewLongText(longText, 80);
  assert.equal(preview.truncated, true);
  assert.ok(preview.preview.length <= 80);
  assert.equal(preview.fullText, longText);
  assert.match(
    chatMenuDisabledReason("chat-test", "chat-test", "run-active", false) ?? "",
    /正在执行/,
  );
});
