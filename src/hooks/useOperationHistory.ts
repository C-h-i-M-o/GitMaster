import { useState, type ChangeEvent } from "react";
import type { OperationHistoryEntry } from "../ui/operationHistory";
import { describeOperationKind } from "../ui/operationPresentation";

/** 每个记录视图独立管理筛选和选择，共享控制器的任务数据。 */
export function useOperationHistory(history: readonly OperationHistoryEntry[]) {
  const [filter, setFilter] = useState("all");
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<string | null>(null);
  const entries = history.filter(
    (item) =>
      (filter === "all" ||
        (filter === "running"
          ? item.outcome === null
          : item.outcome === "failed" || item.outcome === "unknown")) &&
      `${describeOperationKind(item.kind)} ${item.target}`
        .toLocaleLowerCase()
        .includes(query.trim().toLocaleLowerCase()),
  );
  const detail =
    entries.find((item) => item.id === selected) ?? entries[0] ?? null;
  /** 过滤仅改变当前记录标签，不请求或执行任何 Git 操作。 */
  function changeFilter(event: ChangeEvent<HTMLSelectElement>): void {
    setFilter(event.target.value);
  }
  /** 更新本标签的名称查询。 */
  function changeQuery(event: ChangeEvent<HTMLInputElement>): void {
    setQuery(event.target.value);
  }
  /** 用户选中任务后保留详情，不被后续进度抢走焦点。 */
  function select(id: string): () => void {
    return () => setSelected(id);
  }
  return { filter, query, entries, detail, changeFilter, changeQuery, select };
}
