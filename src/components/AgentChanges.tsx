import { lazy, Suspense } from "react";
import type {
  AgentTranscriptChange,
  AgentTranscriptConfiguration,
  AgentTranscriptWarning,
} from "../ui/agentTranscript";
import {
  agentChangeKey,
  agentChangeLabel,
  agentChangeVersionLabel,
  agentConfigurationRows,
} from "../ui/agentChanges";
import { useAgentChangeContent } from "../hooks/useAgentChangeContent";
import "./AgentChanges.css";

const AgentChangeDiff = lazy(() => import("./AgentChangeDiff"));

/** 显示一条不与同路径其他写入合并的历史变更，并按展开状态请求快照。 */
function AgentChangeCard({
  chatId,
  entry,
}: {
  chatId: string;
  entry: AgentTranscriptChange;
}) {
  const { change } = entry;
  const { expanded, loading, content, error, toggle, retry } =
    useAgentChangeContent(chatId, change);
  return (
    <article
      className="agent-change-entry"
      data-agent-change={change.artifactId}
      data-agent-sequence={entry.sequence}
    >
      <button
        type="button"
        className="agent-change-toggle"
        aria-expanded={expanded}
        onClick={toggle}
      >
        <span>{change.path}</span>
        <small>
          {agentChangeLabel(change)} · 事件 {entry.sequence}
        </small>
      </button>
      <p className="agent-change-history-note">
        本次写入时保存的历史快照前后对照，不代表文件当前状态。
      </p>
      <small className="agent-change-versions">
        前：{agentChangeVersionLabel(change.beforeVersion)} · 后：
        {agentChangeVersionLabel(change.afterVersion)}
      </small>
      {expanded && loading && (
        <p className="agent-change-state" role="status">
          正在读取历史快照…
        </p>
      )}
      {expanded && error && (
        <div className="agent-change-error" role="alert">
          <p>{error}</p>
          <button type="button" className="secondary" onClick={retry}>
            重试读取
          </button>
        </div>
      )}
      {expanded && content && (
        <Suspense
          fallback={
            <p className="agent-change-state" role="status">
              正在加载只读差异视图…
            </p>
          }
        >
          <AgentChangeDiff
            artifactId={change.artifactId}
            beforeContent={content.beforeContent ?? ""}
            afterContent={content.afterContent}
          />
        </Suspense>
      )}
    </article>
  );
}

/** 逐条展示由后端记录的文件变更，不按路径合并调用来源。 */
export function AgentChanges({
  chatId,
  changes,
}: {
  chatId: string;
  changes: AgentTranscriptChange[];
}) {
  if (changes.length === 0) return null;
  return (
    <section className="agent-changes" aria-label="本轮文件变更">
      <strong>本轮写入的文件</strong>
      {changes.map((entry) => (
        <AgentChangeCard
          key={agentChangeKey(
            entry.sequence,
            entry.callId,
            entry.change.artifactId,
          )}
          chatId={chatId}
          entry={entry}
        />
      ))}
    </section>
  );
}

/** 显示运行中不阻断操作的后端提示。 */
export function AgentRunWarnings({
  warnings,
}: {
  warnings: AgentTranscriptWarning[];
}) {
  if (warnings.length === 0) return null;
  return (
    <section className="agent-run-warnings" aria-label="运行提示">
      {warnings.map((warning) => (
        <p key={warning.sequence} role="status">
          {warning.message}
        </p>
      ))}
    </section>
  );
}

/** 列出本轮实际应用的参数和模型连接修订号。 */
export function AgentRunConfiguration({
  configurations,
}: {
  configurations: AgentTranscriptConfiguration[];
}) {
  if (configurations.length === 0) return null;
  return (
    <section className="agent-run-configuration">
      {configurations.map((configuration) => (
        <div key={configuration.sequence}>
          <strong>本轮生效参数</strong>
          <p className="agent-run-revisions">
            模型修订：{configuration.modelRevision ?? "未提供"} · 连接修订：
            {configuration.connectionRevision ?? "未提供"}
          </p>
          <dl>
            {agentConfigurationRows(configuration).map((row) => (
              <div key={row.label}>
                <dt>{row.label}</dt>
                <dd>{row.value}</dd>
              </div>
            ))}
          </dl>
        </div>
      ))}
    </section>
  );
}
