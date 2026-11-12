import { createServer, type ServerResponse } from "node:http";
import { mkdtempSync, mkdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { execFileSync } from "node:child_process";

interface Message {
  role: string;
  content: unknown;
  tool_call_id?: string;
}
interface RequestBody {
  messages: Message[];
  tools?: { function: { name: string } }[];
}
const root = mkdtempSync(join(tmpdir(), "gitmaster-release-acceptance-"));
const project = join(root, "project");
mkdirSync(project);
execFileSync("git", ["init", "-b", "main", project], {
  stdio: "ignore",
  windowsHide: true,
});
writeFileSync(
  join(project, "fixture.txt"),
  "仅用于 Release 验收的临时内容\n",
  "utf8",
);
writeFileSync(
  join(root, ".gitmaster-acceptance-owner"),
  "GitMaster Release 临时验收\n",
);
const manifest = resolve("target/iteration-2-release-manifest.json");
const metrics = {
  requests: 0,
  terminal: false,
  git: false,
  skill: false,
  stage: false,
  errors: [] as string[],
};

/** 用标准 Chat Completions SSE 驱动真实适配器，不伪造宿主工具结果。 */
function reply(
  response: ServerResponse,
  tool?: { name: string; id: string; args: Record<string, unknown> },
  text = "RELEASE_ACCEPTANCE_OK",
): void {
  const delta = tool
    ? {
        role: "assistant",
        tool_calls: [
          {
            index: 0,
            id: tool.id,
            type: "function",
            function: { name: tool.name, arguments: JSON.stringify(tool.args) },
          },
        ],
      }
    : { role: "assistant", content: text };
  const chunks = [
    {
      id: "chatcmpl-release-fixture",
      object: "chat.completion.chunk",
      created: 0,
      model: "fixture",
      choices: [{ index: 0, delta, finish_reason: null }],
    },
    {
      id: "chatcmpl-release-fixture",
      object: "chat.completion.chunk",
      created: 0,
      model: "fixture",
      choices: [
        { index: 0, delta: {}, finish_reason: tool ? "tool_calls" : "stop" },
      ],
    },
  ];
  response.writeHead(200, {
    "content-type": "text/event-stream",
    connection: "close",
  });
  response.end(
    chunks.map((chunk) => `data: ${JSON.stringify(chunk)}\n\n`).join("") +
      "data: [DONE]\n\n",
  );
}

/** 只分析本夹具自己的标记，不记录请求正文、请求头或凭据。 */
function toolText(messages: Message[], id: string): string | undefined {
  const message = [...messages]
    .reverse()
    .find((item) => item.role === "tool" && item.tool_call_id === id);
  return message ? JSON.stringify(message.content) : undefined;
}

/** 提供本机假模型、健康状态与精确关闭入口，禁止连接外部服务。 */
const server = createServer(async (request, response) => {
  if (request.url === "/metrics") {
    response.setHeader("content-type", "application/json");
    response.end(JSON.stringify(metrics));
    return;
  }
  if (request.url === "/shutdown") {
    response.end("ok");
    server.close();
    server.closeAllConnections();
    return;
  }
  try {
    let body = "";
    for await (const chunk of request) {
      body += String(chunk);
      if (Buffer.byteLength(body) > 2 * 1024 * 1024)
        throw new Error("FIXTURE_REQUEST_LIMIT");
    }
    const data = JSON.parse(body) as RequestBody;
    metrics.requests++;
    const marker = [...data.messages]
      .reverse()
      .find(
        (message) =>
          message.role === "user" &&
          (JSON.stringify(message.content).includes("RELEASE_PIPELINE") ||
            JSON.stringify(message.content).includes("RELEASE_STAGE")),
      );
    if (!marker) throw new Error("FIXTURE_MISSING_MARKER");
    if (JSON.stringify(marker.content).includes("RELEASE_STAGE")) {
      const result = toolText(data.messages, "release-stage");
      if (result === undefined) {
        reply(response, {
          name: "git_stage",
          id: "release-stage",
          args: { paths: ["fixture.txt"] },
        });
        return;
      }
      metrics.stage = result.includes("succeeded");
      if (!metrics.stage) metrics.errors.push("GIT_STAGE_FAILED");
      reply(
        response,
        undefined,
        metrics.stage ? "RELEASE_STAGE_OK" : "RELEASE_STAGE_FAILED",
      );
      return;
    }
    const terminal = toolText(data.messages, "release-terminal");
    if (terminal === undefined) {
      reply(response, {
        name: process.platform === "win32" ? "pwsh" : "bash",
        id: "release-terminal",
        args: {
          description: "验证临时工作区终端输出",
          command:
            process.platform === "win32"
              ? "Write-Output 'NATIVE_TERMINAL_OK'"
              : "printf NATIVE_TERMINAL_OK",
        },
      });
      return;
    }
    metrics.terminal = terminal.includes("NATIVE_TERMINAL_OK");
    if (!metrics.terminal) throw new Error("TERMINAL_FAILED");
    const git = toolText(data.messages, "release-git");
    if (git === undefined) {
      reply(response, { name: "git_status", id: "release-git", args: {} });
      return;
    }
    metrics.git = git.includes("fixture.txt");
    if (!metrics.git) throw new Error("GIT_STATUS_FAILED");
    const skill = toolText(data.messages, "release-skill");
    if (skill === undefined) {
      reply(response, {
        name: "skill",
        id: "release-skill",
        args: { name: "git-development-summary" },
      });
      return;
    }
    metrics.skill = skill.includes("五个规定章节");
    if (!metrics.skill) throw new Error("SKILL_LOAD_FAILED");
    reply(response);
  } catch (error) {
    metrics.errors.push(
      error instanceof Error ? error.message : "FIXTURE_FAILED",
    );
    reply(response, undefined, "RELEASE_ACCEPTANCE_FAILED");
  }
});
server.listen(0, "127.0.0.1", () => {
  const address = server.address();
  if (!address || typeof address === "string") throw new Error("FIXTURE_PORT");
  const settingsPath = join(root, "settings.json");
  const model = {
    providerId: "release-fixture",
    modelId: "fixture",
    reasoningEffort: null,
  };
  writeFileSync(
    settingsPath,
    JSON.stringify({
      version: 1,
      revision: 0,
      providers: [
        {
          providerId: model.providerId,
          displayName: "本机验收模型",
          protocol: "openai-completions",
          baseUrl: `http://127.0.0.1:${address.port}/v1`,
          credentialRef: null,
          allowInsecureHttp: false,
          models: [
            {
              modelId: "fixture",
              displayName: "验收模型",
              contextWindow: 32768,
              maxOutputTokens: 1024,
            },
          ],
        },
      ],
      defaultModel: model,
      defaultPermission: "workspace-write",
    }),
  );
  writeFileSync(
    manifest,
    JSON.stringify({
      root,
      project,
      settingsPath,
      home: join(root, "home"),
      port: address.port,
    }),
  );
  process.stdout.write(`本机验收服务就绪：port=${address.port}\n`);
});
