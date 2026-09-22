import { useCommitGraph, laneColor } from "../hooks/useCommitGraph";
import type { Workbench } from "../hooks/useWorkbench";
import { Icon } from "./Icon";
import { describeGitError } from "../ui/gitPresentation";
/** 以真实提交和父关系绘制可导航画布，拖动仅改变展示坐标。 */
export function CommitGraph({ workbench: w }: { workbench: Workbench }) {
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
        <h1>每一步，都有迹可循。</h1>
      </div>
      <label className="graph-search">
        <Icon name="search" />
        <input
          aria-label="查找提交或引用"
          placeholder="查找提交、作者或引用"
          value={graph.query}
          onChange={graph.changeQuery}
        />
      </label>
      {w.history.error && (
        <p className="graph-notice" role="alert">
          {describeGitError(w.history.error)}
        </p>
      )}
      {!w.repo.repository ? (
        <div className="graph-empty">
          <Icon name="branch" />
          <h2>从一个项目开始</h2>
          <p>
            {w.preview
              ? "浏览器仅展示界面。请在桌面应用中连接系统 Git。"
              : "打开本地仓库，查看提交与分支之间的联系。"}
          </p>
          <button className="primary" disabled={!w.canOpen} onClick={w.open}>
            打开本地仓库
          </button>
        </div>
      ) : w.history.loading ? (
        <div className="graph-empty" role="status">
          正在读取提交历史…
        </div>
      ) : w.history.page?.commits.length === 0 ? (
        <div className="graph-empty">
          <h2>等待第一份提交</h2>
          <p>在本地修改中暂存文件，然后创建项目的第一个版本。</p>
          <button className="secondary" onClick={w.openDrawer("changes")}>
            查看本地修改
          </button>
        </div>
      ) : null}
      <svg
        className="commit-svg"
        ref={graph.svg}
        aria-label="提交历史图，方向键上下移动，回车查看详情"
        onPointerDown={graph.pointerDown}
        onPointerMove={graph.pointerMove}
        onPointerUp={graph.release}
        onPointerCancel={graph.cancel}
      >
        <g transform={graph.transform}>
          {graph.edges.map((edge) => (
            <path
              key={edge.id}
              d={edge.path}
              className={edge.boundary ? "graph-edge boundary" : "graph-edge"}
            />
          ))}
          {graph.nodes.map((node) => (
            <g
              key={node.oid}
              data-oid={node.oid}
              transform={`translate(${node.x} ${node.y})`}
              tabIndex={0}
              role="button"
              aria-label={`${node.commit.subject}，${node.oid}`}
              aria-pressed={w.history.selectedOid === node.oid}
              onKeyDown={graph.nodeKeyDown}
              className={`graph-node ${node.matches ? "" : "dimmed"} ${w.history.selectedOid === node.oid ? "selected" : ""}`}
            >
              <title>
                {node.commit.subject}
                {"\n"}
                {node.oid}
                {"\n"}
                {node.refs.map((ref) => ref.name).join(" · ")}
              </title>
              <circle r="18" className="node-halo" />
              <circle r="9" fill={laneColor(node.lane)} />
              <circle r="3" fill="white" />
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
              {(w.preferences.saved?.showLabels ?? true) &&
                node.refs.length > 0 && (
                  <text x="27" y="34" className="node-ref">
                    {node.refs.map((ref) => ref.name).join(" · ")}
                  </text>
                )}
            </g>
          ))}
        </g>
      </svg>
      <div className="graph-legend">
        <span className="legend-line" /> 父子关系{" "}
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
          disabled={w.history.loadingMore}
          onClick={w.history.loadMore}
        >
          {w.history.loadingMore ? "正在加载…" : "加载更多提交"}
        </button>
      )}
    </div>
  );
}
