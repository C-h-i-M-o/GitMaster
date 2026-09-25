import type {
  OperationError,
  ReadDocument,
  ReadLine,
  ReadPage,
  ReadSearch,
} from "../types/git";
import { normalizeOperationError } from "../services/gitErrors.ts";

export interface PagedReaderApi {
  open(): Promise<ReadDocument>;
  close(id: string): Promise<void>;
  read(
    id: string,
    start: number,
    count: number,
    offset: number,
  ): Promise<ReadPage>;
  search(id: string, query: string, start: number): Promise<ReadSearch>;
}
export interface PagedReaderState {
  document: ReadDocument | null;
  pages: readonly ReadPage[];
  loading: boolean;
  busy: boolean;
  error: OperationError | null;
}
/** 单文档有界页缓存；迟到打开释放资源，版本错误停止自动补页。 */
export function createPagedReader(api: PagedReaderApi) {
  let state: PagedReaderState = {
    document: null,
    pages: [],
    loading: false,
    busy: false,
    error: null,
  };
  let epoch = 0;
  let desired = { start: 0, end: 0 };
  let running: Promise<void> | null = null;
  const listeners = new Set<() => void>();
  /** 以不可变快照发布有限缓存。 */
  function update(patch: Partial<PagedReaderState>): void {
    state = { ...state, ...patch };
    for (const listener of listeners) listener();
  }
  /** 释放只读能力，旧会话已经销毁也无需向用户报错。 */
  function close(id: string): void {
    void api.close(id).catch(() => {});
  }
  /** 从最近页优先取行，单行续读可以覆盖旧首段。 */
  function line(index: number): ReadLine | undefined {
    for (let i = state.pages.length - 1; i >= 0; i--) {
      const found = state.pages[i]?.lines.find(
        (value) => value.lineNumber === index + 1,
      );
      if (found) return found;
    }
    return undefined;
  }
  /** 单页最多 256 KiB，最多八页；不把正文写入持久存储。 */
  function cache(page: ReadPage): void {
    update({
      pages: [
        ...state.pages.filter((value) => value.startLine !== page.startLine),
        page,
      ].slice(-8),
    });
  }
  /** 关闭同时使在途页和搜索失效；下次打开从磁盘建立新索引。 */
  function dispose(): void {
    epoch++;
    if (state.document) close(state.document.documentId);
    running = null;
    desired = { start: 0, end: 0 };
    update({
      document: null,
      pages: [],
      loading: false,
      busy: false,
      error: null,
    });
  }
  /** 重开只读索引，迟到结果绝不安装到新生命周期。 */
  async function open(): Promise<void> {
    dispose();
    const token = epoch;
    update({ loading: true });
    try {
      const document = await api.open();
      if (token !== epoch) {
        close(document.documentId);
        return;
      }
      update({ document });
    } catch (error: unknown) {
      if (token === epoch) update({ error: normalizeOperationError(error) });
    } finally {
      if (token === epoch) update({ loading: false });
    }
  }
  /** 串行补齐当前视口；短页依据实际行数继续，不假设 count 全部返回。 */
  function ensure(start: number, end: number): Promise<void> {
    const document = state.document;
    if (!document || state.error) return Promise.resolve();
    desired = {
      start: Math.max(0, start),
      end: Math.min(document.lineCount, end),
    };
    if (running) return running;
    const token = epoch;
    update({ busy: true });
    running = (async () => {
      await Promise.resolve();
      try {
        while (token === epoch) {
          let missing = desired.start;
          while (missing < desired.end && line(missing)) missing++;
          if (missing >= desired.end) break;
          const page = await api.read(
            document.documentId,
            missing,
            Math.min(100, desired.end - missing),
            0,
          );
          if (token !== epoch) return;
          if (
            page.documentId !== document.documentId ||
            !page.lines.length ||
            page.lines[0]?.lineNumber !== missing + 1
          )
            throw { code: "STALE_REQUEST" };
          cache(page);
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
  /** 长行只替换当前段，避免续读累积为完整巨型字符串。 */
  async function segment(index: number, offset: number): Promise<void> {
    const document = state.document;
    if (!document || state.busy || state.error) return;
    const token = epoch;
    update({ busy: true });
    running = (async () => {
      try {
        const page = await api.read(document.documentId, index, 1, offset);
        if (token === epoch) cache(page);
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
    if (token === epoch) await ensure(desired.start, desired.end);
  }
  /** 返回下一处匹配行；生命周期改变时丢弃结果。 */
  async function search(query: string, start: number): Promise<number | null> {
    const document = state.document;
    if (!document || state.error) return null;
    const token = epoch;
    try {
      const result = await api.search(document.documentId, query, start);
      return token === epoch ? (result.lines[0] ?? null) : null;
    } catch (error: unknown) {
      if (token === epoch) update({ error: normalizeOperationError(error) });
      return null;
    }
  }
  return {
    open,
    dispose,
    ensure,
    segment,
    search,
    line,
    getSnapshot: () => state,
    subscribe: (listener: () => void) => {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
  };
}
