import {
  useVirtualChanges,
  type VirtualChangesProps,
} from "../hooks/useVirtualChanges";
import { describeStatus } from "../ui/gitPresentation";

/** 虚拟网格保持原复选框与差异按钮分工，完整提交说明在列表外。 */
export function VirtualChanges(props: VirtualChangesProps) {
  const view = useVirtualChanges(props);
  return (
    <div
      ref={view.container}
      className="virtual-changes"
      role="grid"
      aria-label="本地变更文件，空格勾选，回车查看差异"
      aria-multiselectable="true"
      aria-rowcount={view.rows.length}
      aria-colcount={2}
      tabIndex={0}
      aria-activedescendant={view.activeId}
      onKeyDown={view.keyDown}
    >
      <div
        role="presentation"
        style={{ height: view.totalSize, position: "relative" }}
      >
        {view.items.map((item) => {
          const row = view.rows[item.index];
          if (!row) return null;
          return (
            <div
              role="row"
              id={view.rowId(item.index)}
              key={item.key}
              aria-rowindex={item.index + 1}
              aria-selected={
                row.kind === "file"
                  ? props.selected[row.action].includes(row.file.changeId)
                  : undefined
              }
              className={`virtual-change-row ${row.kind === "file" ? "file-row" : ""} ${item.index === view.activeIndex ? "keyboard-active" : ""}`}
              style={{
                position: "absolute",
                top: 0,
                left: 0,
                height: item.size,
                transform: `translateY(${item.start}px)`,
              }}
            >
              {row.kind === "header" ? (
                <div role="gridcell" aria-colspan={2}>
                  <strong>{row.title}</strong> <span>{row.count}</span>
                </div>
              ) : row.kind === "empty" ? (
                <div role="gridcell" aria-colspan={2} className="muted small">
                  没有{row.title}文件
                </div>
              ) : (
                <>
                  <div role="gridcell">
                    <input
                      type="checkbox"
                      tabIndex={-1}
                      aria-label={`${row.action === "stage" ? "暂存" : "取消暂存"} ${row.file.path}`}
                      checked={props.selected[row.action].includes(
                        row.file.changeId,
                      )}
                      disabled={props.blocked || row.file.kind === "submodule"}
                      onChange={view.toggle(row)}
                    />
                  </div>
                  <div role="gridcell" className="virtual-change-content">
                    <button
                      className="file-name"
                      tabIndex={-1}
                      onClick={view.inspect(row)}
                      title={row.file.path}
                    >
                      {row.file.originalPath && (
                        <span className="muted">
                          {row.file.originalPath} →{" "}
                        </span>
                      )}
                      {row.file.path}
                    </button>
                    <span className="file-status">
                      {describeStatus(
                        row.side === "staged"
                          ? row.file.indexStatus
                          : row.file.worktreeStatus,
                      )}
                    </span>
                  </div>
                </>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
