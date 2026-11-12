import assert from "node:assert/strict";
import { createServer, type ServerResponse } from "node:http";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { MessageId } from "@deepseek-ai/dsh-llm";
import { SessionId } from "@deepseek-ai/dsh-session";
import type { AiProtocol } from "./contracts.ts";
import { createRuntime } from "./runtime.ts";

/** 生成手工定义的真实协议事件，用于检验上游适配器接线和工具参数拼接。 */
function reply(
  response: ServerResponse,
  protocol: AiProtocol,
  tool: boolean,
): void {
  response.writeHead(200, { "content-type": "text/event-stream" });
  /** 仅输出本测试服务器的 SSE 帧，不涉及生产解析实现。 */
  const send = (data: unknown, event?: string): void => {
    response.write(
      `${event ? `event: ${event}\n` : ""}data: ${JSON.stringify(data)}\n\n`,
    );
  };
  const argumentsText = JSON.stringify({ file_path: "fixture.txt" });
  if (protocol === "openai-completions") {
    const base = {
      id: "chatcmpl-fixture",
      object: "chat.completion.chunk",
      created: 0,
      model: "fixture",
    };
    if (tool) {
      send({
        ...base,
        choices: [
          {
            index: 0,
            delta: {
              role: "assistant",
              tool_calls: [
                {
                  index: 0,
                  id: "read-1",
                  type: "function",
                  function: {
                    name: "read",
                    arguments: argumentsText.slice(0, 12),
                  },
                },
              ],
            },
            finish_reason: null,
          },
        ],
      });
      send({
        ...base,
        choices: [
          {
            index: 0,
            delta: {
              tool_calls: [
                { index: 0, function: { arguments: argumentsText.slice(12) } },
              ],
            },
            finish_reason: null,
          },
        ],
      });
    } else
      send({
        ...base,
        choices: [
          {
            index: 0,
            delta: { role: "assistant", content: "协议工具循环完成" },
            finish_reason: null,
          },
        ],
      });
    send({
      ...base,
      choices: [
        { index: 0, delta: {}, finish_reason: tool ? "tool_calls" : "stop" },
      ],
      usage: { prompt_tokens: 20, completion_tokens: 10, total_tokens: 30 },
    });
    response.end("data: [DONE]\n\n");
    return;
  }
  if (protocol === "anthropic-messages") {
    send(
      {
        type: "message_start",
        message: {
          id: "msg_fixture",
          type: "message",
          role: "assistant",
          model: "fixture",
          content: [],
          stop_reason: null,
          stop_sequence: null,
          usage: { input_tokens: 20, output_tokens: 0 },
        },
      },
      "message_start",
    );
    send(
      {
        type: "content_block_start",
        index: 0,
        content_block: tool
          ? { type: "tool_use", id: "read-1", name: "read", input: {} }
          : { type: "text", text: "" },
      },
      "content_block_start",
    );
    if (tool) {
      for (const partial_json of [
        argumentsText.slice(0, 12),
        argumentsText.slice(12),
      ])
        send(
          {
            type: "content_block_delta",
            index: 0,
            delta: { type: "input_json_delta", partial_json },
          },
          "content_block_delta",
        );
    } else
      send(
        {
          type: "content_block_delta",
          index: 0,
          delta: { type: "text_delta", text: "协议工具循环完成" },
        },
        "content_block_delta",
      );
    send({ type: "content_block_stop", index: 0 }, "content_block_stop");
    send(
      {
        type: "message_delta",
        delta: {
          stop_reason: tool ? "tool_use" : "end_turn",
          stop_sequence: null,
        },
        usage: { output_tokens: 10 },
      },
      "message_delta",
    );
    send({ type: "message_stop" }, "message_stop");
    response.end();
    return;
  }
  const base = {
    id: "resp_fixture",
    object: "response",
    created_at: 0,
    model: "fixture",
    status: "in_progress",
    output: [],
  };
  send({ type: "response.created", response: base }, "response.created");
  const item = tool
    ? {
        type: "function_call",
        id: "fc_fixture",
        call_id: "read-1",
        name: "read",
        arguments: "",
      }
    : { type: "message", id: "msg_fixture", role: "assistant", content: [] };
  send(
    { type: "response.output_item.added", output_index: 0, item },
    "response.output_item.added",
  );
  if (tool) {
    for (const delta of [argumentsText.slice(0, 12), argumentsText.slice(12)])
      send(
        {
          type: "response.function_call_arguments.delta",
          item_id: "fc_fixture",
          output_index: 0,
          delta,
        },
        "response.function_call_arguments.delta",
      );
    send(
      {
        type: "response.function_call_arguments.done",
        item_id: "fc_fixture",
        output_index: 0,
        arguments: argumentsText,
      },
      "response.function_call_arguments.done",
    );
  } else {
    send(
      {
        type: "response.content_part.added",
        item_id: "msg_fixture",
        output_index: 0,
        content_index: 0,
        part: { type: "output_text", text: "", annotations: [] },
      },
      "response.content_part.added",
    );
    send(
      {
        type: "response.output_text.delta",
        item_id: "msg_fixture",
        output_index: 0,
        content_index: 0,
        delta: "协议工具循环完成",
      },
      "response.output_text.delta",
    );
  }
  const completed = tool
    ? { ...item, arguments: argumentsText, status: "completed" }
    : {
        ...item,
        content: [
          { type: "output_text", text: "协议工具循环完成", annotations: [] },
        ],
        status: "completed",
      };
  send(
    { type: "response.output_item.done", output_index: 0, item: completed },
    "response.output_item.done",
  );
  send(
    {
      type: "response.completed",
      response: {
        ...base,
        status: "completed",
        output: [completed],
        usage: {
          input_tokens: 20,
          output_tokens: 10,
          total_tokens: 30,
          input_tokens_details: { cached_tokens: 0 },
          output_tokens_details: { reasoning_tokens: 0 },
        },
      },
    },
    "response.completed",
  );
  response.end();
}

for (const protocol of [
  "openai-completions",
  "openai-responses",
  "anthropic-messages",
] satisfies AiProtocol[]) {
  test(
    `${protocol} 完成真实 HTTP 分片工具参数与结果回填`,
    { timeout: 20000 },
    async () => {
      const root = await mkdtemp(join(tmpdir(), "gitmaster-protocol-"));
      const requests: string[] = [];
      const server = createServer(async (request, response) => {
        const chunks: Buffer[] = [];
        for await (const chunk of request) chunks.push(Buffer.from(chunk));
        const body = Buffer.concat(chunks).toString("utf8");
        requests.push(body);
        reply(response, protocol, requests.length === 1);
      });
      await new Promise<void>((resolve) =>
        server.listen(0, "127.0.0.1", resolve),
      );
      const address = server.address();
      assert.ok(address && typeof address !== "string");
      const runtime = await createRuntime({
        workspaceRoot: root,
        providers: [
          {
            providerId: "fixture-gateway",
            displayName: "测试网关",
            protocol,
            baseUrl: `http://127.0.0.1:${address.port}/v1`,
            credentialRef: "GITMASTER_AI_TEST",
            allowInsecureHttp: false,
            models: [
              {
                modelId: "fixture",
                displayName: "测试模型",
                contextWindow: 8192,
                maxOutputTokens: 1024,
              },
            ],
          },
        ],
        credentials: { resolve: async () => "temporary-test-key" },
      });
      try {
        await writeFile(join(root, "fixture.txt"), "真实协议读取内容", "utf8");
        const handle = await runtime.ctx.agents.create({
          sessionId: SessionId(`fixture-${protocol}`),
          meta: { cwd: root },
          agentOptions: { provider: "fixture-gateway", model: "fixture" },
        });
        try {
          handle.agent.followup({
            role: "user",
            id: MessageId("input"),
            source: { kind: "user" },
            content: [{ type: "text", text: "读取文件" }],
          });
          await handle.agent.whenIdle();
          assert.equal(
            requests.length,
            2,
            JSON.stringify(handle.agent.session.snapshotEvents()).slice(-2000),
          );
          assert.match(requests[1] ?? "", /真实协议读取内容/);
          assert.match(
            JSON.stringify(handle.agent.session.snapshotEvents()),
            /协议工具循环完成/,
          );
          assert.ok(
            requests.every((body) => !body.includes("temporary-test-key")),
          );
        } finally {
          await handle.dispose();
        }
      } finally {
        await runtime.ctx.fiber.dispose();
        server.closeAllConnections();
        await new Promise<void>((resolve) => server.close(() => resolve()));
        await rm(root, { recursive: true, force: true });
      }
    },
  );
}
