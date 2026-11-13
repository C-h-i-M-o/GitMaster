import { createRoot } from "react-dom/client";
import { mockIPC } from "@tauri-apps/api/mocks";
import { useState } from "react";
import { useAiWorkspace } from "../../../../src/hooks/useAiWorkspace";
import type { Workbench } from "../../../../src/hooks/useWorkbench";
import AiWorkspace from "../../../../src/components/ai/AiWorkspace";
import { AiSandboxSettings } from "../../../../src/components/ai/AiSandboxSettings";
import type {
  AiEventEnvelope,
  AiModelTestResult,
  AiSession,
} from "../../../../src/types/ai";
import "../../../../src/styles.css";

const model = {
  providerId: "fixture",
  modelId: "model",
  reasoningEffort: null,
};
let mode = "completed";
let requests = 0;
let reads = 0;
let hidePartial = false;
let epochReads = 0;
let rejectRead: (() => void) | undefined;
let releaseTest: ((value: AiModelTestResult) => void) | undefined;
Object.assign(window, { isTauri: true });
/** 本地 IPC 仅返回合成状态，不执行模型请求、命令或真实设置保存。 */
mockIPC(async (command, args) => {
  if (command === "read_ai_settings")
    return {
      settings: {
        version: 1,
        revision: 1,
        defaultModel: model,
        defaultPermission: "workspace-write",
        providers: [
          {
            providerId: "fixture",
            displayName: "本机测试",
            protocol: "openai-completions",
            baseUrl: "http://127.0.0.1:9/v1",
            credentialRef: null,
            allowInsecureHttp: false,
            models: [
              {
                modelId: "model",
                displayName: "测试模型",
                contextWindow: 8192,
                maxOutputTokens: 1024,
              },
            ],
          },
        ],
      },
      credentials: {},
    };
  if (command === "list_ai_sessions") return { items: [], nextOffset: null };
  if (command === "read_ai_session") {
    reads++;
    if (mode === "delayed")
      return new Promise((_, reject) => {
        Object.assign(window, { pendingRead: true });
        rejectRead = () => {
          Object.assign(window, { pendingRead: false });
          reject({ code: "AI_TIMEOUT" });
        };
      });
    if (mode === "read-error") throw { code: "AI_TIMEOUT" };
    const sessionId = String(args?.sessionId);
    const after = Number(args?.after);
    const session: AiSession = {
      sessionId,
      workspaceRoot: "C:/fixture",
      repositoryKey: "fixture",
      title: "回归对话",
      model,
      permission: "workspace-write",
      createdAt: "2026-11-13",
      updatedAt: "2026-11-13",
    };
    const all: AiEventEnvelope[] = [
      {
        sequence: 1,
        sessionId,
        runtimeEpoch: "epoch",
        runId: null,
        turn: 1,
        event: {
          kind: "error",
          code: "AI_NETWORK_ERROR",
          message: "模型服务连接失败（UND_ERR_CONNECT_TIMEOUT）",
        },
      },
      {
        sequence: 2,
        sessionId,
        runtimeEpoch: "epoch",
        runId: null,
        turn: 1,
        event: { kind: "turn", turn: 1, state: "failed" },
      },
      ...(mode === "completed" || mode.startsWith("epoch")
        ? [
            {
              sequence: 3,
              sessionId,
              runtimeEpoch: "epoch",
              runId: null,
              turn: 2,
              event: {
                kind: "assistant" as const,
                messageId: "reply",
                text: "后续轮次已经正常完成。",
                final: true,
              },
            },
            {
              sequence: 4,
              sessionId,
              runtimeEpoch: "epoch",
              runId: null,
              turn: 2,
              event: {
                kind: "turn" as const,
                turn: 2,
                state: "completed" as const,
              },
            },
          ]
        : []),
    ];
    if (mode === "awaiting")
      all.push(
        {
          sequence: 3,
          sessionId,
          runtimeEpoch: "epoch",
          runId: "active",
          turn: 2,
          event: { kind: "turn", turn: 2, state: "running" },
        },
        {
          sequence: 4,
          sessionId,
          runtimeEpoch: "epoch",
          runId: "active",
          turn: 2,
          event: {
            kind: "approval",
            approvalId: "approval",
            title: "确认测试",
            detail: "只用于状态恢复",
            withdrawn: false,
          },
        },
      );
    if (mode === "awaiting" && !hidePartial)
      all.push({
        sequence: 5,
        sessionId,
        runtimeEpoch: "epoch",
        runId: "active",
        turn: 2,
        event: {
          kind: "assistant",
          messageId: "stream",
          text: "已经显示的流式内容",
          final: false,
        },
      });
    const runtimeEpoch =
      mode === "missing-meta"
        ? ""
        : mode === "epoch-repeat"
          ? `epoch-${++epochReads}`
          : mode === "epoch-once"
            ? ++epochReads === 1
              ? "old"
              : "new"
            : "epoch";
    const events = all
      .filter((row) => row.sequence > after)
      .slice(0, 2)
      .map((row) => ({ ...row, runtimeEpoch }));
    return {
      session,
      events,
      status: mode === "awaiting" ? "awaitingApproval" : "idle",
      runtimeEpoch,
      activeRunId: mode === "awaiting" ? "active" : null,
      nextSequence: events.at(-1)?.sequence ?? after,
      more: (events.at(-1)?.sequence ?? after) < all.length,
    };
  }
  if (command === "read_ai_activity") return { busy: mode === "awaiting" };
  if (command === "cancel_ai_run") {
    Object.assign(window, { cancelledRun: args?.runId });
    mode = "completed";
    return;
  }
  if (command === "read_ai_jobs") return [];
  if (command === "subscribe_ai_events") return;
  if (command === "test_ai_provider") {
    requests++;
    return new Promise<AiModelTestResult>((resolve) => {
      releaseTest = resolve;
    });
  }
  if (command === "read_ai_sandbox_status")
    return {
      ready: true,
      root: "\\\\?\\C:\\fixture\\长项目路径\\用于排版检查",
      operation: "ready",
      win32Code: null,
      shellAvailable: true,
    };
  throw new Error(`夹具拒绝请求：${command}`);
});
/** 使用真实工作区 hook 与组件，覆盖异步读取和恢复按钮而非静态模型副本。 */
function Harness() {
  const [settings, setSettings] = useState(false);
  const w = {
    aiVisible: true,
    preview: false,
    repo: { repository: { repositoryId: "fixture" } },
    appSettings: {
      draft: {
        uiPreferences: {
          aiChat: { transcriptView: "standard", collapseTiming: "completion" },
        },
      },
    },
    editor: { dirty: false, savingPath: null },
    setAiRunning: () => {},
    refreshLocal: async () => {},
    selectSettings: () => () => setSettings(true),
    openModal: () => () => {},
  } as unknown as Workbench;
  const f = useAiWorkspace(w);
  Object.assign(window, {
    fixtureReselect: () => {
      if (f.session) f.select(f.session)();
    },
  });
  Object.assign(window, { fixtureAwaiting: () => choose("awaiting") });
  Object.assign(window, {
    fixtureHidePartial: () => {
      hidePartial = true;
    },
  });
  Object.assign(window, {
    fixtureEpoch: (repeat: boolean) => {
      epochReads = 0;
      choose(repeat ? "epoch-repeat" : "epoch-once");
    },
    fixtureEpochReads: () => epochReads,
    fixtureDelayed: () => choose("delayed"),
    fixtureRejectRead: () => rejectRead?.(),
    fixtureMissingMeta: () => choose("missing-meta"),
  });
  /** 每次选择均有新身份，触发真实恢复过程并隔离上一会话的迟到结果。 */
  function choose(next: string): void {
    mode = next;
    f.select({
      sessionId: crypto.randomUUID(),
      workspaceRoot: "C:/fixture",
      repositoryKey: "fixture",
      title: "回归对话",
      model,
      permission: "workspace-write",
      createdAt: "2026-11-13",
      updatedAt: "2026-11-13",
    })();
  }
  return (
    <>
      <div style={{ display: "flex", gap: 8, minHeight: 48 }}>
        <button onClick={() => choose("completed")}>加载成功历史</button>
        <button onClick={() => choose("failed")}>加载失败历史</button>
        <button onClick={() => choose("read-error")}>模拟读取失败</button>
        <button
          onClick={() => {
            mode = "completed";
          }}
        >
          恢复读取
        </button>
        <button onClick={() => releaseTest?.({ ok: true })}>
          完成连接测试
        </button>
        <button
          onClick={() =>
            releaseTest?.({
              ok: false,
              error: {
                code: "AI_NETWORK_ERROR",
                message: "本次连接失败（UND_ERR_CONNECT_TIMEOUT）",
              },
            })
          }
        >
          连接失败
        </button>
        <button onClick={() => setSettings(!settings)}>查看权限设置</button>
        <output aria-label="测试统计">
          {JSON.stringify({ requests, reads })}
        </output>
      </div>
      {settings ? (
        <div className="app-shell">
          <div className="settings-dialog" style={{ padding: 24 }}>
            <AiSandboxSettings repositoryId="fixture" />
          </div>
        </div>
      ) : (
        <div
          style={{ height: "calc(100vh - 48px)", display: "grid", padding: 12 }}
        >
          <AiWorkspace f={f} />
        </div>
      )}
    </>
  );
}
createRoot(document.getElementById("root")!).render(<Harness />);
