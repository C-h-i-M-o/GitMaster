import type { DiffPage, DiffRow } from "../types/diff";
import type { OperationError } from "../types/git";
import { normalizeOperationError } from "../services/gitErrors.ts";

interface PageState {
  rows: ReadonlyMap<number, DiffRow>;
  busy: boolean;
  error: OperationError | null;
}
/** 单份差异的有界视口缓存；文档打开关闭由仓库控制器统一管理。 */
export function createDiffPages(
  rowCount: number,
  read: (start: number, count: number, offset: number) => Promise<DiffPage>,
) {
  let state: PageState = { rows: new Map(), busy: false, error: null };
  let epoch = 0;
  let active = false;
  let running: Promise<void> | null = null;
  let desired: readonly number[] = [];
  const listeners = new Set<() => void>();
  /** 发布不可变快照，支持 React 外部状态订阅。 */
  function update(patch: Partial<PageState>): void {
    state = { ...state, ...patch };
    for (const listener of listeners) listener();
  }
  /** 只保留有限正文行，不将可见窗口拼成完整差异。 */
  function row(index: number): DiffRow | undefined {
    return state.rows.get(index);
  }
  /** 最多 128 个行段、2 MiB 文本；优先淘汰不属于当前视口的旧行。 */
  function cache(page: DiffPage): void {
    const rows = new Map(state.rows);
    page.rows.forEach((value, index) => {
      const key = page.startRow + index;
      rows.delete(key);
      rows.set(key, value);
    });
    const encoder = new TextEncoder();
    let bytes = Array.from(rows.values()).reduce(
      (total, value) => total + encoder.encode(value.text).length,
      0,
    );
    for (const [key, value] of rows) {
      if (rows.size <= 128 && bytes <= 2 * 1024 * 1024) break;
      if (desired.includes(key)) continue;
      rows.delete(key);
      bytes -= encoder.encode(value.text).length;
    }
    update({ rows });
  }
  /** 生命周期结束使未决请求失效，严格模式重新激活时从空缓存读取。 */
  function dispose(): void {
    active = false;
    epoch++;
    running = null;
    desired = [];
    update({ rows: new Map(), busy: false, error: null });
  }
  /** 显式开始当前组件生命周期，不复用之前失败状态。 */
  function activate(): void {
    dispose();
    active = true;
  }
  /** 校验短页确实从请求位置开始，避免无法前进的补页循环。 */
  async function load(
    start: number,
    count: number,
    offset: number,
    token: number,
  ): Promise<void> {
    const page = await read(start, count, offset);
    if (!active || token !== epoch) return;
    if (
      page.startRow !== start ||
      !page.rows.length ||
      page.rows.length > count ||
      page.rows[0]?.byteOffset !== offset
    )
      throw { code: "STALE_REQUEST" };
    cache(page);
  }
  /** 串行补齐当前视口；最大视口不足 96 行，避免缓存容量小于请求范围。 */
  function ensure(start: number, end: number): Promise<void> {
    if (!active || state.error) return Promise.resolve();
    desired = Array.from(
      {
        length: Math.max(
          0,
          Math.min(rowCount, end, Math.max(0, start) + 96) - Math.max(0, start),
        ),
      },
      (_, i) => Math.max(0, start) + i,
    );
    return ensureRows(desired);
  }
  /** 折叠视口只请求实际可见的原始行，不读取隐藏区间。 */
  function ensureRows(indices: readonly number[]): Promise<void> {
    if (!active || state.error) return Promise.resolve();
    desired = [...new Set(indices.filter((i) => i >= 0 && i < rowCount))].slice(
      0,
      96,
    );
    if (running) return running;
    const token = epoch;
    update({ busy: true });
    running = (async () => {
      await Promise.resolve();
      try {
        while (active && token === epoch) {
          const position = desired.findIndex((index) => !row(index));
          if (position < 0) break;
          const missing = desired[position]!;
          let count = 1;
          while (
            position + count < desired.length &&
            desired[position + count] === missing + count &&
            !row(missing + count)
          )
            count++;
          await load(missing, count, 0, token);
        }
      } catch (error: unknown) {
        if (token === epoch) update({ error: normalizeOperationError(error) });
      } finally {
        if (token === epoch) {
          running = null;
          update({ busy: false });
        }
      }
    })();
    return running;
  }
  /** 长行续段和视口补页共享同一串行通道。 */
  async function segment(index: number, offset: number): Promise<void> {
    if (!active || state.busy || state.error) return;
    const token = epoch;
    update({ busy: true });
    running = (async () => {
      try {
        await load(index, 1, offset, token);
      } catch (error: unknown) {
        if (token === epoch) update({ error: normalizeOperationError(error) });
      } finally {
        if (token === epoch) {
          running = null;
          update({ busy: false });
        }
      }
    })();
    await running;
    if (active && token === epoch) await ensureRows(desired);
  }
  return {
    activate,
    dispose,
    ensure,
    ensureRows,
    segment,
    row,
    getSnapshot: () => state,
    subscribe: (listener: () => void) => {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
  };
}
