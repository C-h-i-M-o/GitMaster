import assert from "node:assert/strict";
import test, { after } from "node:test";
import { build } from "vite";
import { createElement, type ReactElement, type ReactNode } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { rm } from "node:fs/promises";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import type { AgentEvent, EventPayload, ProviderProfile } from "../src/types/agent.ts";
import type { AgentFileChangeContent } from "../src/types/agentRun.ts";
import { projectAgentTranscript } from "../src/ui/agentTranscript.ts";
import { agentChangeContentMatches } from "../src/ui/agentChanges.ts";

/** 构造指定会话中带有稳定序号的持久事件。 */
function event(sequence: number, payload: EventPayload): AgentEvent {
  return {
    version: 1,
    sequence,
    sessionId: "chat-a11",
    runId: "run-a11",
    timestamp: sequence,
    ...payload,
  };
}

const profile: ProviderProfile = {
  id: "profile-a11",
  name: "本地测试",
  protocol: "responses",
  baseUrl: "https://provider.example/v1",
  model: "test-model",
  credentialId: "credential-a11",
};

/** 创建两个同路径但来源调用和工件都不同的历史变更事件。 */
function transcriptEvents(): AgentEvent[] {
  const change = (artifactId: string, digest: string, afterVersion: string) => ({
    artifactId,
    digest,
    path: "src/cache.ts",
    beforeVersion: "before-version",
    afterVersion,
    created: false,
  });
  return [
    event(1, { kind: "runStarted", profile, permissionMode: "requestApproval" }),
    event(2, { kind: "userMessage", text: "请修改缓存逻辑" }),
    event(3, {
      kind: "fileChanged",
      callId: "call-first",
      change: change("artifact-first", "digest-first", "after-first"),
    }),
    event(4, {
      kind: "fileChanged",
      callId: "call-second",
      change: change("artifact-second", "digest-second", "after-second"),
    }),
    event(5, { kind: "runWarning", message: "部分上下文已截断。" }),
    event(6, {
      kind: "runConfiguration",
      parameters: {
        contextLimit: 12000,
        maxOutputTokens: 1200,
        thinkingMode: "enabled",
        reasoningEffort: "high",
        thinkingBudgetTokens: 800,
        temperature: 0.4,
        topP: null,
      },
      modelRevision: 7,
      connectionRevision: 4,
    }),
    event(7, {
      kind: "assistantMessage",
      text: "已按要求修改。",
      protocol: "responses",
      wire: { output: [] },
      calls: [],
    }),
    event(8, { kind: "runEnded", status: "completed", message: null }),
    event(9, { kind: "memorySuggested", text: "保留缓存策略", tags: ["缓存"] }),
    event(10, {
      kind: "userReferences",
      references: [{
        kind: "file",
        label: "src/cache.ts 第 8-9 行",
        path: "src/cache.ts",
        startLine: 8,
        endLine: 9,
        version: "historical-version",
        text: "历史引用文本",
      }],
    }),
    event(11, {
      kind: "modelAttempt",
      purpose: "reply",
      attempt: {
        id: "attempt-a11",
        attempt: 1,
        phase: "completed",
        usage: {
          inputTokens: 120,
          outputTokens: 18,
          cachedInputTokens: null,
          cacheCreationInputTokens: null,
          reasoningTokens: null,
        },
        errorCode: null,
      },
    }),
  ];
}

let agentTurnComponent:
  | typeof import("../src/components/AgentTurn").AgentTurn
  | null = null;
let agentRunContextComponent:
  | typeof import("../src/components/AgentRunContext").AgentRunContext
  | null = null;
const ssrOutputDirectory = resolve("node_modules/.vite-temp/agent-changes-ssr");
const contextOutputDirectory = resolve("node_modules/.vite-temp/agent-context-ssr");

/** 用实际 AgentTurn TSX 与 ReactDOMServer 检查最终 HTML。 */
async function renderTurn(
  turn: ReturnType<typeof projectAgentTranscript>[number],
  onMemorySaved?: () => void,
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
    createElement(agentTurnComponent, {
      turn,
      activeRun: null,
      workspaceId: "default",
      onMemorySaved,
    }),
  );
}

test("每次 fileChanged 独立保留，警告和运行配置不合并丢失", () => {
  const events = transcriptEvents();
  const [turn] = projectAgentTranscript([...events, events[2]!]);
  assert.ok(turn);
  assert.equal(turn.changes.length, 2);
  assert.equal(turn.changes[0]?.callId, "call-first");
  assert.equal(turn.changes[1]?.callId, "call-second");
  assert.equal(turn.changes[0]?.change.path, turn.changes[1]?.change.path);
  assert.deepEqual(turn.warnings.map((warning) => warning.message), ["部分上下文已截断。"]);
  assert.equal(turn.configurations.length, 1);
  assert.equal(turn.configurations[0]?.modelRevision, 7);
  assert.equal(turn.userSequence, 2);
  assert.equal(turn.hasProcess, true);
  assert.equal(turn.memorySuggestions[0]?.sequence, 9);
  assert.equal(turn.references[0]?.references[0]?.version, "historical-version");
});

test("快照只接受后端已验证且与事件版本和路径一致的内容", () => {
  const [turn] = projectAgentTranscript(transcriptEvents());
  const change = turn?.changes[0]?.change;
  assert.ok(change);
  const valid: AgentFileChangeContent = {
    path: change.path,
    beforeContent: "const cache = 1;",
    afterContent: "const cache = 2;",
    beforeVersion: change.beforeVersion,
    afterVersion: change.afterVersion,
    verified: true,
  };
  assert.equal(agentChangeContentMatches(change, valid), true);
  assert.equal(agentChangeContentMatches(change, { ...valid, verified: false }), false);
  assert.equal(agentChangeContentMatches(change, { ...valid, path: "other.ts" }), false);
  assert.equal(
    agentChangeContentMatches(change, { ...valid, afterVersion: "unmatched" }),
    false,
  );
});

test("真实 SSR 展示历史变更、警告和实际参数，并输出消息锚点", async () => {
  const [turn] = projectAgentTranscript(transcriptEvents());
  assert.ok(turn);
  const markup = await renderTurn(turn);
  assert.match(markup, /本次写入时保存的历史快照前后对照/);
  assert.match(markup, /不代表文件当前状态/);
  assert.match(markup, /部分上下文已截断。/);
  assert.match(markup, /模型修订：7/);
  assert.match(markup, /连接修订：4/);
  assert.match(markup, /最高输出 token<\/dt><dd>1200/);
  assert.match(markup, /推理强度<\/dt><dd>high/);
  assert.equal(markup.match(/class="agent-change-entry"/g)?.length, 2);
  assert.match(markup, /artifact-first/);
  assert.match(markup, /artifact-second/);
  assert.match(markup, /id="agent-message-chat-a11-2"/);
  assert.match(markup, /data-agent-sequence="2"/);
  assert.match(markup, /id="agent-message-chat-a11-7"/);
  assert.match(markup, /data-agent-sequence="7"/);
  assert.doesNotMatch(markup, /monaco-diff-editor/);
  assert.match(markup, /本轮用量/);
  assert.match(markup, /输入 120/);
  assert.match(markup, /保存此记忆/);
  assert.match(markup, /历史引用文本/);
  assert.match(markup, /不代表文件当前内容/);
});

test("记忆建议成功回调从轮次上下文传到保存卡片", async () => {
  if (!agentRunContextComponent) {
    await build({
      configFile: false,
      build: {
        ssr: resolve("src/components/AgentRunContext.tsx"),
        outDir: contextOutputDirectory,
        emptyOutDir: true,
        minify: false,
        rollupOptions: {
          output: { entryFileNames: "agent-run-context.mjs", format: "es" },
        },
      },
    });
    const module = await import(
      pathToFileURL(resolve(contextOutputDirectory, "agent-run-context.mjs")).href
    );
    agentRunContextComponent = module.AgentRunContext;
  }
  const onSaved = () => {};
  const tree = agentRunContextComponent({
    chatId: "chat-a11",
    workspaceId: "default",
    suggestions: [{ sequence: 9, text: "保留缓存策略", tags: ["缓存"] }],
    referenceGroups: [],
    onMemorySaved: onSaved,
  });
  /** 在React元素树中找到真实的记忆建议组件并核对回调透传。 */
  function findSuggestion(
    node: ReactNode,
  ): ReactElement<{ onSaved?: () => void }> | null {
    if (Array.isArray(node)) {
      for (const child of node) {
        const found = findSuggestion(child);
        if (found) return found;
      }
      return null;
    }
    if (node === null || typeof node !== "object") return null;
    const element = node as ReactElement<{
      children?: ReactNode;
      chatId?: string;
      sequence?: number;
      onSaved?: () => void;
    }>;
    if (!element.props) return null;
    if ("chatId" in element.props && "sequence" in element.props)
      return element;
    const children = element.props.children;
    if (Array.isArray(children)) {
      for (const child of children) {
        const found = findSuggestion(child);
        if (found) return found;
      }
    }
    return findSuggestion(children);
  }
  const suggestion = findSuggestion(tree);
  assert.ok(suggestion);
  assert.equal(suggestion.props.onSaved, onSaved);
});

after(async () => {
  await rm(ssrOutputDirectory, { recursive: true, force: true });
  await rm(contextOutputDirectory, { recursive: true, force: true });
  agentTurnComponent = null;
  agentRunContextComponent = null;
});
