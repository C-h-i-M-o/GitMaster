import { useCommitGraph } from "../hooks/useCommitGraph";
import { graphLaneStyle } from "../ui/graphColors";
import type { Workbench } from "../hooks/useWorkbench";
import { Icon } from "./Icon";
import { BranchHeads } from "./BranchHeads";
import { describeGitError } from "../ui/gitPresentation";
import { shortcutLabel } from "../ui/shortcuts";
import { OperationFeedback } from "./OperationFeedback";
import { graphFeedback, commitFeedback } from "../ui/operationFeedback";
/** 以真实提交和父关系绘制可导航画布，拖动仅改变展示坐标。 */
export function CommitGraph({ workbench: w }: { workbench: Workbench }) {
  const feedback =
    (w.drawer === null ? commitFeedback(w) : null) ?? graphFeedback(w);
  const graph = useCommitGraph(
    w.history.page,
    w.history.selectedOid,
    w.selectCommit,
    w.preferences.saved?.elasticity ?? 6,
  );
  return (
    <div className="graph-canvas" ref={graph.root}>
      <div className="graph-heading">
        <span className="eyebrow">COMMIT HISTORY</span>
      </div>
      <label className="graph-search">
        <Icon name="search" />
        <input
          aria-label="查找提交或引用"
          title={`查找已加载的提交（${shortcutLabel("F")}）`}
          placeholder="查找提交、作者或引用"
          value={graph.query}
          onChange={graph.changeQuery}
          onKeyDown={graph.searchKeyDown}
        />
      </label>
      {graph.query.trim() && (
        <div className="graph-search-results" aria-live="polite">
          <span>已加载历史中有 {graph.matches.length} 个匹配提交</span>
          <span>
            {graph.matches.length
              ? `${graph.matchIndex + 1}/${graph.matches.length}`
              : "0/0"}
          </span>
          <button
            type="button"
            disabled={!graph.matches.length}
            onClick={graph.previousMatch}
            aria-label="上一个匹配提交"
            title="上一个匹配提交（Shift+Enter）"
          >
            上一项
          </button>
          <button
            type="button"
            disabled={!graph.matches.length}
            onClick={graph.nextMatch}
            aria-label="下一个匹配提交"
            title="下一个匹配提交（Enter）"
          >
            下一项
          </button>
          {!!graph.matches.length && (
            <details>
              <summary>查看匹配结果</summary>
              <ul>
                {graph.matches.map((node, index) => (
                  <li key={node.oid}>
                    <button
                      type="button"
                      aria-current={index === graph.matchIndex}
                      onClick={graph.selectMatch(node.oid)}
                    >
                      {node.commit.subject}
                    </button>
                  </li>
                ))}
              </ul>
            </details>
          )}
        </div>
      )}
      {w.history.error && !feedback?.error && (
        <p className="graph-notice" role="alert">
          {describeGitError(w.history.error)}
        </p>
      )}
      {!w.repo.repository && !feedback && !w.repo.openingPath ? (
        <div className="graph-empty">
          <Icon name="branch" />
          <h2>未打开项目</h2>
          <button className="primary" disabled={!w.canOpen} onClick={w.open}>
            打开本地仓库
          </button>
        </div>
      ) : !feedback && w.history.page?.commits.length === 0 ? (
        <div className="graph-empty">
          <h2>暂无提交</h2>
          <button className="secondary" onClick={w.openDrawer("changes")}>
            查看本地修改
          </button>
        </div>
      ) : null}
      {feedback && (
        <div className="graph-feedback">
          <OperationFeedback model={feedback} />
        </div>
      )}
      <svg
        className="commit-svg"
        ref={graph.svg}
        aria-label="提交历史图，方向键上下移动，回车查看详情"
        onPointerDown={graph.pointerDown}
        onPointerMove={graph.pointerMove}
        onPointerUp={graph.release}
        onPointerCancel={graph.cancel}
      >
        <defs>
          <radialGradient id="bubble-glass" cx="32%" cy="25%" r="75%">
            <stop offset="0%" stopColor="white" stopOpacity="0.95" />
            <stop offset="55%" stopColor="white" stopOpacity="0.18" />
            <stop offset="100%" stopColor="white" stopOpacity="0.04" />
          </radialGradient>
        </defs>
        <g data-graph-world="" transform={graph.transform}>
          {graph.edges.map((edge) => (
            <path
              key={edge.id}
              data-edge={edge.id}
              style={graphLaneStyle(edge.lane)}
              className={edge.boundary ? "graph-edge boundary" : "graph-edge"}
            />
          ))}
          {graph.nodes.map((node) => (
            <g
              key={node.oid}
              data-oid={node.oid}
              style={graphLaneStyle(node.lane)}
              transform={`translate(${node.x} ${node.y})`}
              role="group"
              onKeyDown={graph.nodeKeyDown}
              className={`graph-node ${node.matches ? "" : "dimmed"} ${w.history.selectedOid === node.oid || graph.activeMatchOid === node.oid ? "selected" : ""}`}
            >
              <g
                tabIndex={w.history.stale ? -1 : 0}
                aria-disabled={w.history.stale}
                role="button"
                aria-label={`${node.commit.subject}，${node.oid}`}
                aria-pressed={w.history.selectedOid === node.oid}
                className="graph-node-action"
              >
                <title>
                  {node.commit.subject}
                  {"\n"}
                  {node.oid}
                  {"\n"}
                  {node.refs.map((ref) => ref.name).join(" · ")}
                </title>
                <circle r="23" className="node-halo" />
                <circle r="16" className="node-core" />
                <circle
                  r="19"
                  className="node-glass"
                  fill="url(#bubble-glass)"
                />
                <circle cx="-6" cy="-7" r="4" fill="white" fillOpacity="0.85" />
                {(w.preferences.saved?.showLabels ?? true) && (
                  <text x="27" y="-7" className="node-subject">
                    {node.commit.subject.length > 29
                      ? `${node.commit.subject.slice(0, 29)}…`
                      : node.commit.subject}
                  </text>
                )}
                <text x="27" y="13" className="node-meta">
                  {node.oid.slice(0, 7)} · {node.commit.authorName}
                </text>
              </g>
              <BranchHeads
                refs={node.refs}
                head={w.repo.repository?.head}
                oid={node.oid}
              />
            </g>
          ))}
        </g>
      </svg>
      <div className="graph-legend">
        <span className="legend-line" /> 分支线{" "}
        <span className="legend-line dashed" /> 尚未加载的父提交
      </div>
      <div className="graph-controls">
        <button title="缩小" aria-label="缩小" onClick={graph.zoomOut}>
          <Icon name="minus" />
        </button>
        <span>{Math.round(graph.view.zoom * 100)}%</span>
        <button title="放大" aria-label="放大" onClick={graph.zoomIn}>
          <Icon name="plus" />
        </button>
        <button
          title="回到选中提交"
          aria-label="回到选中提交"
          onClick={graph.center}
        >
          <Icon name="center" />
        </button>
      </div>
      {w.history.page?.nextCursor && (
        <button
          className="load-more secondary"
          disabled={w.history.loadingMore || w.history.stale}
          onClick={w.history.loadMore}
        >
          {w.history.loadingMore ? "正在加载…" : "加载更多提交"}
        </button>
      )}
    </div>
  );
}
