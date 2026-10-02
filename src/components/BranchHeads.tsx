import { useBranchHeads } from "../hooks/useBranchHeads";
import type { HeadState, HistoryPage } from "../types/git";

/** 用半透明标签展示真实引用，保留完整名称与独立 HEAD 标识。 */
export function BranchHeads({
  refs,
  head,
  oid,
}: {
  refs: HistoryPage["tips"];
  head: HeadState | undefined;
  oid: string;
}) {
  const state = useBranchHeads(refs, head, oid);
  if (!state.heads.length) return null;
  return (
    <foreignObject
      x={-state.size.width / 2}
      y={-state.size.height - 28}
      width={state.size.width}
      height={state.size.height}
      className="branch-head-object"
    >
      <div
        ref={state.container}
        className={`branch-heads ${state.expanded ? "expanded" : ""}`}
        onPointerDown={state.stop}
        onKeyDown={state.stop}
      >
        {state.visible.map((item) => (
          <span
            className={`branch-head ${item.kind} ${item.current ? "current" : ""}`}
            key={item.id}
            title={`${item.kind === "remote" ? "远端" : item.kind === "local" ? "本地" : ""} ${item.name}${item.current ? " · HEAD" : ""}`}
            aria-label={`${item.kind === "remote" ? "远端分支" : item.kind === "local" ? "本地分支" : "分离头指针"} ${item.name}${item.current ? "，当前 HEAD" : ""}`}
            tabIndex={0}
          >
            <span className="branch-head-kind">
              {item.kind === "remote" ? "↗" : "⑂"}
            </span>
            <span className="branch-head-name">{item.name}</span>
            {item.current && <small>HEAD</small>}
          </span>
        ))}
        {state.heads.length > 2 && (
          <button
            className="branch-head-more"
            onClick={state.toggle}
            aria-expanded={state.expanded}
            aria-label={
              state.expanded
                ? "收起分支引用"
                : `查看全部 ${state.heads.length} 个引用`
            }
          >
            {state.expanded ? "收起" : `+${state.heads.length - 2}`}
          </button>
        )}
      </div>
    </foreignObject>
  );
}
