import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import type { AiWorkspaceController, AiRow } from "../../hooks/useAiWorkspace";
import { useAiTheme } from "../../hooks/useAiTheme";
import { Icon } from "../Icon";
import { AiModelSelect, AiPermissionSelect } from "./AiSelectors";
import { aiJobDetail, aiJobStateLabel } from "../../ui/aiPresentation";
import brandMark from "../../../assets/branding/production/app-icon.png";
import "./upstream-theme.css";
import "./ai.css";
import { useAiDisclosure } from "../../hooks/useAiDisclosure";
import { aiLiveDetail } from "../../ui/aiTurns";

/** 工具与审批按照上游过程行呈现，文本始终通过安全 Markdown 渲染。 */
function AiMessage({ row, f }: { row: AiRow; f: AiWorkspaceController }) {
  const event = row.event;
  if (event.kind === "turn") return null;
  if (event.kind === "user")
    return <div className="ai-user-message">{event.text}</div>;
  if (event.kind === "assistant")
    return (
      <article className="ai-assistant-message">
        <ReactMarkdown remarkPlugins={[remarkGfm]}>{event.text}</ReactMarkdown>
      </article>
    );
  if (event.kind === "tool")
    return (
      <details
        className={`ai-tool ${event.failed ? "ai-tool-error" : ""}`}
        open={event.failed}
      >
        <summary>
          <Icon
            name={
              event.output === null
                ? "activity"
                : event.failed
                  ? "close"
                  : "check"
            }
          />
          <span>{event.name}</span>
          <span className="ai-caption">
            {event.output === null
              ? "执行中"
              : event.state === "cancelled"
                ? "已取消"
                : event.state === "interrupted"
                  ? "已中断"
                  : event.failed
                    ? "已失败"
                    : "已完成"}
          </span>
        </summary>
        <pre>{event.input}</pre>
        {event.output !== null && <pre>{event.output}</pre>}
      </details>
    );
  if (event.kind === "approval")
    return (
      <section className="ai-approval">
        <strong>允许执行 {event.title}？</strong>
        <pre>{event.detail}</pre>
        <div className="ai-form-actions">
          <button onClick={f.approve(event.approvalId, "rejected")}>
            拒绝
          </button>
          <button
            className="ai-primary"
            onClick={f.approve(event.approvalId, "allowed-once")}
          >
            允许一次
          </button>
        </div>
      </section>
    );
  if (event.kind === "question")
    return (
      <section className="ai-approval">
        <strong>需要你的回答</strong>
        {f.questions(event.detail).map((question) => (
          <div className="ai-question" key={question.id}>
            <label htmlFor={`${event.questionId}/${question.id}`}>
              {question.question}
            </label>
            <div className="ai-question-options">
              {question.options.map((option) => (
                <button
                  key={option.label}
                  aria-pressed={
                    f.selections[
                      `${event.questionId}/${question.id}`
                    ]?.includes(option.label) ?? false
                  }
                  onClick={f.selectAnswer(
                    event.questionId,
                    question,
                    option.label,
                  )}
                  title={option.description}
                >
                  {option.label}
                </button>
              ))}
            </div>
            <input
              id={`${event.questionId}/${question.id}`}
              value={f.answers[`${event.questionId}/${question.id}`] ?? ""}
              onChange={f.answerField(event.questionId, question)}
              placeholder="填写回答；留空表示跳过"
            />
          </div>
        ))}
        <button
          className="ai-primary"
          onClick={f.answer(event.questionId, event.detail)}
        >
          提交回答
        </button>
      </section>
    );
  if (event.kind === "error")
    return (
      <p className="ai-error" role="alert">
        {event.message}
      </p>
    );
  return null;
}

/** 推理与工具属于可折叠过程，助手正文始终保留在过程之外。 */
function AiProcessRow({
  row,
  f,
  preview,
}: {
  row: AiRow;
  f: AiWorkspaceController;
  preview: boolean;
}) {
  if (row.event.kind === "assistant")
    return row.event.reasoning ? (
      <details className="ai-reasoning">
        <summary>
          思考过程
          {preview && (
            <span className="ai-caption">
              {row.event.reasoning.split("\n")[0]}
            </span>
          )}
        </summary>
        <pre>{row.event.reasoning}</pre>
      </details>
    ) : null;
  return (
    <>
      <AiMessage row={row} f={f} />
      {f.presentation.transcriptView !== "verbose" &&
        f.presentation.transcriptView !== "compact" &&
        row.event.kind === "tool" &&
        row.event.output === null && (
          <pre className="ai-live-detail">{row.event.input}</pre>
        )}
    </>
  );
}
/** 按上游规则控制单步骤分组；父级收起后子控件卸载并重置。 */
function AiStep({
  rows,
  f,
  open,
  version,
}: {
  rows: AiRow[];
  f: AiWorkspaceController;
  open: boolean;
  version: string;
}) {
  const disclosure = useAiDisclosure(open, version);
  return (
    <details
      className="ai-process-step"
      open={disclosure.open}
      onToggle={disclosure.toggle}
    >
      <summary>
        {rows.some((row) => row.event.kind === "tool")
          ? `${rows.filter((row) => row.event.kind === "tool").length} 项工作步骤`
          : "思考过程"}
        {(f.presentation.transcriptView === "standard" ||
          f.presentation.transcriptView === "detailed") && (
          <code className="ai-step-preview">{aiLiveDetail(rows)}</code>
        )}
      </summary>
      {disclosure.open &&
        rows.map((row) => (
          <AiProcessRow
            key={row.key}
            row={row}
            f={f}
            preview={f.presentation.transcriptView !== "compact"}
          />
        ))}
    </details>
  );
}
/** 仅正常完成的整轮可自动收起，审批、问题和所有助手回复保持可见。 */
function AiTurn({
  turn,
  f,
}: {
  turn: AiWorkspaceController["turns"][number];
  f: AiWorkspaceController;
}) {
  const disclosure = useAiDisclosure(!turn.policy.foldTurn, turn.resetVersion);
  const process =
    turn.policy.stepGrouping === "none"
      ? turn.process.map((row) => (
          <AiProcessRow
            key={row.key}
            row={row}
            f={f}
            preview={turn.policy.settledReasoningPreview}
          />
        ))
      : turn.steps.map(([step, rows], index) => (
          <AiStep
            key={step}
            rows={rows}
            f={f}
            open={
              turn.policy.stepGrouping === "history" &&
              turn.state === "running" &&
              index === turn.steps.length - 1
            }
            version={`${turn.resetVersion}-${turn.state}-${index === turn.steps.length - 1}`}
          />
        ));
  return (
    <section className="ai-turn">
      {turn.messages
        .filter((row) => row.event.kind === "user")
        .map((row) => (
          <AiMessage key={row.key} row={row} f={f} />
        ))}
      {turn.process.length > 0 &&
        (turn.policy.canFoldTurn ? (
          <details
            className="ai-turn-process"
            open={disclosure.open}
            onToggle={disclosure.toggle}
          >
            <summary>工作步骤</summary>
            {disclosure.open && process}
          </details>
        ) : (
          <div className="ai-turn-process">{process}</div>
        ))}
      {turn.messages
        .filter((row) => row.event.kind !== "user")
        .map((row) => (
          <AiMessage key={row.key} row={row} f={f} />
        ))}
    </section>
  );
}
/** AI 工作区保留上游会话轴、输入卡片及右侧任务面板的视觉结构。 */
export default function AiWorkspace({ f }: { f: AiWorkspaceController }) {
  const dark = useAiTheme();
  return (
    <section
      className="ai-surface ai-workspace"
      data-ai-dark={dark ? "" : undefined}
      aria-label="AI 工作区"
    >
      <div className="ai-conversation">
        <header className="ai-conversation-header">
          <button
            className="ai-title"
            onClick={f.renameSession}
            disabled={!f.session || f.busy}
          >
            {f.session?.title ?? "新会话"}
          </button>
          <div className="ai-header-actions">
            <button
              onClick={f.compact}
              disabled={!f.session || f.busy}
              title="压缩上下文"
            >
              压缩
            </button>
            <button onClick={f.toggleTerminal} aria-pressed={f.terminalOpen}>
              终端
            </button>
            <button onClick={f.settings} aria-label="模型设置">
              <Icon name="settings" />
            </button>
          </div>
        </header>
        <div
          className={`ai-conversation-body ${f.rows.length ? "" : "ai-hero"}`}
        >
          {f.rows.length ? (
            <div
              className="ai-transcript"
              ref={f.transcript}
              onScroll={f.scroll}
              aria-live="polite"
            >
              <div className="ai-message-column">
                {f.turns.map((turn) => (
                  <AiTurn key={turn.turn} turn={turn} f={f} />
                ))}
              </div>
            </div>
          ) : (
            <div className="ai-empty">
              <div className="ai-empty-headline">
                <img className="ai-hero-mark" src={brandMark} alt="" />
                <h2>今天一起完成什么？</h2>
              </div>
              <p>
                {f.preview
                  ? "请在桌面应用中使用 AI。"
                  : !f.repositoryId
                    ? "先打开一个项目，开始与 AI 协作。"
                    : !f.validModel
                      ? "配置并选择模型，然后开始对话。"
                      : "阅读项目、修改代码，或运行终端命令。"}
              </p>
              {!f.validModel && <button onClick={f.settings}>配置模型</button>}
            </div>
          )}
          <div className="ai-composer-stack">
            {f.error && (
              <p className="ai-error" role="alert">
                {f.error}
                <button type="button" onClick={f.reconnect}>
                  重新载入会话
                </button>
              </p>
            )}
            <div className="ai-composer-card">
              <textarea
                value={f.draft}
                onChange={f.changeDraft}
                onCompositionStart={f.composeDraft}
                onKeyDown={f.composerKey}
                placeholder="向 GitMaster 提问，或描述要完成的任务"
                aria-label="AI 消息"
                disabled={f.preview || !f.repositoryId || !f.validModel}
                rows={3}
              />
              <div className="ai-composer-toolbar">
                <AiModelSelect f={f} />
                <AiPermissionSelect f={f} />
                {f.efforts && (
                  <select
                    aria-label="推理强度"
                    value={f.reasoningEffort}
                    onChange={f.chooseEffort}
                    disabled={f.busy}
                  >
                    <option value="">默认推理</option>
                    {Object.keys(f.efforts).map((effort) => (
                      <option key={effort} value={effort}>
                        {effort}
                      </option>
                    ))}
                  </select>
                )}
                {f.busy ? (
                  <button
                    className="ai-send"
                    aria-label="停止生成"
                    title={f.status === "stopping" ? "正在停止…" : "停止生成"}
                    disabled={f.status === "stopping"}
                    onClick={f.stop}
                  >
                    <svg
                      width="10"
                      height="10"
                      viewBox="0 0 10 10"
                      aria-hidden="true"
                    >
                      <rect width="10" height="10" rx="1" fill="currentColor" />
                    </svg>
                  </button>
                ) : (
                  <button
                    className="ai-send"
                    aria-label="发送消息"
                    disabled={
                      !f.draft.trim() ||
                      !f.validModel ||
                      !f.repositoryId ||
                      f.preview
                    }
                    onClick={f.send}
                  >
                    ↑
                  </button>
                )}
              </div>
            </div>
            <div className="ai-composer-status">
              {f.status === "awaitingApproval"
                ? "等待你的批准"
                : f.status === "awaitingAnswer"
                  ? "等待你的回答"
                  : f.status === "stopping"
                    ? "正在停止…"
                    : f.busy
                      ? "正在处理…"
                      : "Enter 发送 · Shift + Enter 换行"}
            </div>
          </div>
        </div>
      </div>
      {f.terminalOpen && (
        <aside className="ai-terminal">
          <header>
            <strong>AI 终端</strong>
            <button
              onClick={f.stopAllJobs}
              disabled={!f.jobs.some((job) => job.state === "running")}
            >
              全部停止
            </button>
            <button aria-label="关闭任务面板" onClick={f.toggleTerminal}>
              <Icon name="close" />
            </button>
          </header>
          <div className="ai-terminal-body">
            {!f.jobs.length && (
              <p className="ai-caption">命令运行后，任务与输出会显示在这里。</p>
            )}
            {f.jobs.map((job) => (
              <section className="ai-job" key={job.jobId}>
                <div>
                  <strong>{job.label}</strong>
                  <span className="ai-caption">
                    {aiJobStateLabel(job.state)}
                  </span>
                  <button
                    onClick={f.stopJob(job.jobId)}
                    disabled={job.state !== "running"}
                  >
                    停止
                  </button>
                </div>
                <p className="ai-caption">{job.workingDirectory}</p>
                <pre>{job.command}</pre>
                <pre>{job.output || "暂无输出"}</pre>
                {job.detail && (
                  <p className="ai-caption">{aiJobDetail(job.detail)}</p>
                )}
              </section>
            ))}
          </div>
        </aside>
      )}
      <dialog
        className="ai-risk-dialog"
        ref={f.permissionDialog.dialogRef}
        onCancel={f.permissionDialog.onCancel}
      >
        <h3>启用完全访问？</h3>
        <p>
          AI
          可以修改当前账户可访问的文件并运行命令，不再由工作区文件沙箱限制。此选择影响当前会话和新会话默认值。
        </p>
        <label className="ai-risk-acknowledge">
          <input
            type="checkbox"
            checked={f.riskAcknowledged}
            onChange={f.acknowledgeRisk}
          />
          我了解完全访问允许 AI 在工作区以外修改文件和运行命令。
        </label>
        <div className="ai-form-actions">
          <button onClick={f.cancelFullAccess}>取消</button>
          <button
            className="ai-primary"
            onClick={f.confirmFullAccess}
            disabled={!f.riskAcknowledged}
          >
            启用完全访问
          </button>
        </div>
      </dialog>
    </section>
  );
}
