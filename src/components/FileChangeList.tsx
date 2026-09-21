import type { FileChange, DiffSide } from "../types/git";
import { describeStatus } from "../ui/gitPresentation";
interface Props {
  changes: FileChange[];
  side: DiffSide;
  visible: number;
  onSelect: (id: string, side: DiffSide) => () => void;
  onMore: () => void;
  disabled: boolean;
  title?: string;
}
/** 展示单个状态分组及重命名、机器状态的中文说明。 */
export function FileChangeList({
  changes,
  side,
  visible,
  onSelect,
  onMore,
  disabled,
  title,
}: Props) {
  return (
    <div>
      <h3>
        {title ??
          (side === "staged"
            ? "已暂存"
            : side === "unstaged"
              ? "未暂存"
              : side === "untracked"
                ? "未跟踪"
                : "冲突")}{" "}
        <small>{changes.length}</small>
      </h3>
      {changes.slice(0, visible).map((change) => (
        <button
          className="change-item"
          type="button"
          disabled={disabled}
          key={`${side}-${change.changeId}`}
          onClick={onSelect(change.changeId, side)}
        >
          {change.originalPath
            ? `${change.originalPath} → ${change.path}`
            : change.path}
          <small>
            {" "}
            · {describeStatus(change.indexStatus)}/
            {describeStatus(change.worktreeStatus)}
          </small>
        </button>
      ))}
      {changes.length > visible && (
        <button type="button" onClick={onMore}>
          加载更多（剩余 {changes.length - visible}）
        </button>
      )}
    </div>
  );
}
