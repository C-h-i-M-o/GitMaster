import type { useAgentContext } from "../hooks/useAgentContext";
import { AgentUsage } from "./AgentUsage";
import "./agentContext.css";

/** 提供用户选择的路径、文本、Git 差异引用及发送前上下文预览。 */
export function AgentContextPanel({
  context,
  enabled,
  running,
  workspaceId,
  hasChat,
  hasText,
  configurationDirty,
}: {
  context: ReturnType<typeof useAgentContext>;
  enabled: boolean;
  running: boolean;
  workspaceId: string;
  hasChat: boolean;
  hasText: boolean;
  configurationDirty: boolean;
}) {
  return (
    <>
      <button
        type="button"
        className="secondary"
        onClick={context.show}
        disabled={!enabled}
        aria-expanded={context.open}
      >
        @ 上下文与引用
      </button>
      {context.open && (
        <section className="agent-context-panel" aria-label="引用与上下文预览">
          <header>
            <strong>当前工作区：{workspaceId}</strong>
            <button type="button" className="secondary" onClick={context.close}>
              收起
            </button>
          </header>
          <p className="muted">
            最多 8 项、总量 64 KiB。文件和 Git
            差异由后端按当前工作区读取；发送时会再次核对版本。
          </p>
          {configurationDirty && (
            <p role="status">
              当前聊天配置有未保存更改；先保存聊天配置再预览。
            </p>
          )}
          <div className="agent-context-file-fields">
            <label>
              工作区相对路径
              <input
                value={context.path}
                onChange={context.pathChanged}
                placeholder="src/main.ts"
                disabled={running || context.referenceBusy}
              />
            </label>
            <label>
              起始行（可选）
              <input
                type="number"
                min={1}
                value={context.startLine}
                onChange={context.startLineChanged}
                disabled={running || context.referenceBusy}
              />
            </label>
            <label>
              结束行（可选）
              <input
                type="number"
                min={1}
                value={context.endLine}
                onChange={context.endLineChanged}
                disabled={running || context.referenceBusy}
              />
            </label>
          </div>
          <button
            type="button"
            className="secondary"
            onClick={context.addFileReference}
            disabled={running || context.referenceBusy || !enabled}
          >
            添加文件引用并校验
          </button>
          <label className="agent-context-text">
            明确选择或粘贴文本
            <textarea
              value={context.textSelection}
              onChange={context.textSelectionChanged}
              rows={3}
              disabled={running || context.referenceBusy}
            />
          </label>
          <button
            type="button"
            className="secondary"
            onClick={context.addTextReference}
            disabled={running || context.referenceBusy || !enabled}
          >
            添加文本引用
          </button>
          <label className="agent-context-check">
            <input
              type="checkbox"
              checked={context.staged}
              onChange={context.stagedChanged}
              disabled={running || context.referenceBusy}
            />
            只引用已暂存差异
          </label>
          <button
            type="button"
            className="secondary"
            onClick={context.addDiffReference}
            disabled={running || context.referenceBusy || !enabled}
          >
            添加{context.staged ? "暂存区" : "未暂存"}差异
          </button>
          {context.referenceBusy && <p role="status">正在验证所选引用…</p>}
          {context.referenceError && (
            <p role="alert">{context.referenceError}</p>
          )}
          {context.references.length > 0 && (
            <ol className="agent-context-reference-list">
              {context.references.map((reference, index) => {
                const verified = context.verified[index];
                return (
                  <li
                    key={
                      reference.kind +
                      ":" +
                      (reference.path ?? index) +
                      ":" +
                      index
                    }
                  >
                    <div>
                      <strong>
                        {reference.kind === "gitDiff"
                          ? reference.staged
                            ? "暂存区差异"
                            : "未暂存差异"
                          : (verified?.label ?? reference.path ?? "文本引用")}
                      </strong>
                      {verified ? (
                        <>
                          <small>
                            版本 {verified.version}
                            {verified.startLine !== null &&
                            verified.startLine !== undefined
                              ? " · 行 " +
                                verified.startLine +
                                (verified.endLine ? "-" + verified.endLine : "")
                              : ""}
                          </small>
                          <pre>{verified.text}</pre>
                        </>
                      ) : (
                        <small>等待后端校验</small>
                      )}
                    </div>
                    <button
                      type="button"
                      className="secondary"
                      onClick={context.removeReference(index)}
                      disabled={running || context.referenceBusy}
                    >
                      移除
                    </button>
                  </li>
                );
              })}
            </ol>
          )}
          <button
            type="button"
            className="secondary"
            onClick={context.requestContextPreview}
            disabled={
              !enabled ||
              running ||
              !hasText ||
              context.contextBusy ||
              configurationDirty ||
              !context.canSendReferences
            }
          >
            {context.contextBusy
              ? "正在预览…"
              : hasChat
                ? "预览当前输入上下文"
                : "创建聊天并预览"}
          </button>
          {!hasChat && (
            <small>此操作会创建空聊天；不会发送模型请求或添加聊天消息。</small>
          )}
          {context.contextBusy && (
            <p role="status">正在估算上下文与记忆选择…</p>
          )}
          {context.contextError && <p role="alert">{context.contextError}</p>}
          {context.contextPreview && (
            <AgentUsage
              attempts={[]}
              context={context.contextPreview}
              label="发送前上下文估算"
            />
          )}
        </section>
      )}
    </>
  );
}
