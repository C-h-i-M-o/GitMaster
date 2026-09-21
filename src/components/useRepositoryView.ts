import { useCallback, useState } from "react";
import type { DiffSide } from "../types/git";
/** 管理文件列表分页，切换快照后恢复首批 200 条。 */
export function useRepositoryView(
  onSelect: (id: string, side: DiffSide) => void,
  snapshotId: string | null,
) {
  const [page, setPage] = useState({ snapshotId, visible: 200 });
  const visible = page.snapshotId === snapshotId ? page.visible : 200;
  /** 将文件身份与比较侧绑定到展示事件。 */
  const select = useCallback(
    (id: string, side: DiffSide) => () => onSelect(id, side),
    [onSelect],
  );
  /** 每次显式追加一批，不让上一个仓库的页数影响新仓库。 */
  const more = useCallback(
    () =>
      setPage((old) => ({
        snapshotId,
        visible: (old.snapshotId === snapshotId ? old.visible : 200) + 200,
      })),
    [snapshotId],
  );
  return { visible, select, more };
}
