import {
  useEffect,
  useRef,
  useState,
  useSyncExternalStore,
  type ChangeEvent,
  type FormEvent,
} from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { createPagedReader } from "./pagedReaderController";
import * as api from "../services/reader";

export interface PagedReaderProps {
  scope: api.ReaderScope;
  fileId: string;
}
/** 只读视口、跳行与查找行为；调用方按文件能力设置组件 key。 */
export function usePagedReader({ scope, fileId }: PagedReaderProps) {
  const [controller] = useState(() =>
    createPagedReader({
      open: () => api.openReadDocument(scope, fileId),
      close: (id) => api.closeReadDocument(scope, id),
      read: (id, start, count, offset) =>
        api.readDocumentPage(scope, id, start, count, offset),
      search: (id, query, start) =>
        api.searchReadDocument(scope, id, query, start),
    }),
  );
  const state = useSyncExternalStore(
    controller.subscribe,
    controller.getSnapshot,
  );
  const container = useRef<HTMLDivElement>(null);
  const [lineInput, setLineInput] = useState("1");
  const [query, setQuery] = useState("");
  const [notice, setNotice] = useState("");
  const [searching, setSearching] = useState(false);
  const cursor = useRef(0);
  const searchSequence = useRef(0);
  const virtual = useVirtualizer({
    count: state.document?.lineCount ?? 0,
    getScrollElement: () => container.current,
    estimateSize: () => 28,
    overscan: 6,
  });
  const items = virtual.getVirtualItems();
  const start = items[0]?.index ?? 0;
  const end = (items[items.length - 1]?.index ?? -1) + 1;
  useEffect(() => {
    void controller.open();
    return () => {
      searchSequence.current++;
      controller.dispose();
    };
  }, [controller]);
  useEffect(() => {
    if (end > start) void controller.ensure(Math.max(0, start - 10), end + 10);
  }, [controller, state.document, start, end]);
  /** 行号输入不触发加载，提交时才校验并定位。 */
  function changeLine(event: ChangeEvent<HTMLInputElement>): void {
    setLineInput(event.target.value);
  }
  /** 查询变化使在途查找和续查位置失效。 */
  function changeQuery(event: ChangeEvent<HTMLInputElement>): void {
    searchSequence.current++;
    cursor.current = 0;
    setSearching(false);
    setNotice("");
    setQuery(event.target.value);
  }
  /** 虚拟定位不会读取目标之前的正文。 */
  function jump(event: FormEvent<HTMLFormElement>): void {
    event.preventDefault();
    const number = Number(lineInput);
    if (
      !Number.isSafeInteger(number) ||
      number < 1 ||
      number > (state.document?.lineCount ?? 0)
    ) {
      setNotice("请输入文件范围内的整数行号。");
      return;
    }
    virtual.scrollToIndex(number - 1, { align: "start" });
    cursor.current = number - 1;
    setNotice(`已定位第 ${number} 行。`);
  }
  /** 查找为大小写敏感的字面量，每次定位下一个不同逻辑行。 */
  async function find(event: FormEvent<HTMLFormElement>): Promise<void> {
    event.preventDefault();
    if (searching || !state.document) return;
    if (
      !query ||
      new TextEncoder().encode(query).length > 1024 ||
      /[\r\n]/.test(query)
    ) {
      setNotice("请输入不超过 1024 字节的单行查找文本。");
      return;
    }
    const token = ++searchSequence.current;
    setSearching(true);
    const found = await controller.search(query, cursor.current);
    if (token !== searchSequence.current) return;
    setSearching(false);
    if (found !== null) {
      virtual.scrollToIndex(found - 1, { align: "center" });
      setLineInput(String(found));
      cursor.current = found < state.document.lineCount ? found : 0;
      setNotice(`匹配位于第 ${found} 行（同一行仅定位一次）。`);
    } else {
      cursor.current = 0;
      setNotice("已到文件末尾，没有更多匹配；再次查找将从开头开始。");
    }
  }
  /** 显式重读新版本；原错误不会触发自动重试循环。 */
  function reload(): void {
    searchSequence.current++;
    setSearching(false);
    cursor.current = 0;
    setNotice("");
    void controller.open();
  }
  /** 长行续读替换当前段，保持内存有界。 */
  function segment(index: number, offset: number): () => void {
    return () => {
      void controller.segment(index, offset);
    };
  }
  return {
    ...state,
    container,
    items,
    totalSize: virtual.getTotalSize(),
    line: controller.line,
    lineInput,
    query,
    notice,
    searching,
    changeLine,
    changeQuery,
    jump,
    find,
    reload,
    segment,
  };
}
