import type { Workbench } from "../hooks/useWorkbench";
import { useOperationHistory } from "../hooks/useOperationHistory";
import {
  describeOperationKind,
  describeRecoveryPathStatus,
  describeOperationPhase,
  formatOperationExpiry,
} from "../ui/operationPresentation";
import { describeGitError } from "../ui/gitPresentation";

/** 多任务记录按列表与详情呈现，失败保留诊断和恢复路径。 */
export function OperationHistoryPanel({
  workbench: w,
  selectedOperationId,
}: {
  workbench: Workbench;
  selectedOperationId?: string | null;
}) {
  const state = useOperationHistory(w.operations.history, selectedOperationId);
  const detail = state.detail;
  return (
    <div className="operation-history">
      <div className="history-filters">
        <input
          value={state.query}
          onChange={state.changeQuery}
          placeholder="查找操作或项目…"
          aria-label="查找操作记录"
        />
        <select
          value={state.filter}
          onChange={state.changeFilter}
          aria-label="筛选操作状态"
          title="保留本次会话最近 200 条已结束任务"
        >
          <option value="all">全部</option>
          <option value="running">进行中</option>
          <option value="failed">失败或结果未知</option>
        </select>
      </div>
      <div className="history-columns">
        <div className="history-list" aria-label="任务列表">
          {state.entries.map((item) => (
            <button
              className={`history-row ${detail?.id === item.id ? "active" : ""}`}
              key={item.id}
              onClick={state.select(item.id)}
              aria-pressed={detail?.id === item.id}
            >
              <strong>{describeOperationKind(item.kind)}</strong>
              <span className={`history-status ${item.outcome ?? "running"}`}>
                {item.refreshError
                  ? "状态核实失败"
                  : describeOperationPhase(item.phase)}
              </span>
              <small title={item.target}>{item.target}</small>
              <time>{formatOperationExpiry(item.startedAt)}</time>
            </button>
          ))}
          {!state.entries.length && (
            <p className="muted">
              {w.operations.history.length
                ? "没有符合条件的操作。"
                : "本次会话暂无操作。"}
            </p>
          )}
        </div>
        <div className="history-detail" aria-label="任务详情">
          {detail ? (
            <>
              <h3>
                {describeOperationKind(detail.kind)}{" "}
                <small>{describeOperationPhase(detail.phase)}</small>
              </h3>
              <p>{detail.target}</p>
              <p className="small muted">
                {formatOperationExpiry(detail.startedAt)}
                {detail.finishedAt !== null &&
                  ` · 用时 ${Math.max(0, (detail.finishedAt - detail.startedAt) / 1000).toFixed(1)} 秒`}
              </p>
              {detail.outcome === "succeeded" &&
                detail.needsMergeCommit &&
                !detail.refreshError && (
                  <p className="success-text">
                    合并内容已准备，请检查并完成合并提交。
                  </p>
                )}
              {detail.outcome === "unknown" && (
                <p role="alert">
                  结果未知，请核对状态，不要重复执行。
                  {w.operations.handle?.operationId === detail.id && (
                    <button onClick={w.operations.resume}>重新查询任务</button>
                  )}
                </p>
              )}
              {detail.error && (
                <p role="alert">{describeGitError(detail.error)}</p>
              )}
              {detail.refreshError && (
                <p role="alert">
                  操作已结束，刷新失败：{describeGitError(detail.refreshError)}
                </p>
              )}
              {detail.commitOid && <code>{detail.commitOid}</code>}
              {detail.pathResults && (
                <section aria-label="逐文件结果">
                  <h4>文件结果</h4>
                  <ul>
                    {detail.pathResults.map((file) => (
                      <li key={file.path}>
                        {file.path}：{describeRecoveryPathStatus(file.status)}
                        {file.error && ` · ${describeGitError(file.error)}`}
                      </li>
                    ))}
                  </ul>
                  {(detail.pathResultCount ?? 0) >
                    detail.pathResults.length && (
                    <p>
                      共 {detail.pathResultCount} 项，此处保留前 100
                      项；请核对仓库实际状态。
                    </p>
                  )}
                </section>
              )}
              {detail.recoveryPath && <p>保留目录：{detail.recoveryPath}</p>}
              {detail.unresolvedCount !== null && (
                <p>还有 {detail.unresolvedCount} 个冲突，请在合并流程处理。</p>
              )}
              <ol className="history-events">
                {detail.events.map((event) => (
                  <li key={event.sequence}>
                    <span>{describeOperationPhase(event.phase)}</span>
                    {event.counts && (
                      <small>
                        {event.counts.completed}
                        {event.counts.total === null
                          ? ""
                          : ` / ${event.counts.total}`}
                      </small>
                    )}
                  </li>
                ))}
              </ol>
            </>
          ) : (
            <p className="muted">未选择操作</p>
          )}
        </div>
      </div>
    </div>
  );
}
