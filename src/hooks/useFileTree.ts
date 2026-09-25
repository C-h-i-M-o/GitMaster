import { useEffect, useMemo, useRef, useState, type ChangeEvent } from "react";
import type { ProjectTreePage } from "../types/git";
import type { FileTreeNode } from "../ui/fileTree";
import { normalizeOperationError } from "../services/gitErrors";
import { describeGitError } from "../ui/gitPresentation";

/** 目录展开按需获取页面，折叠保留缓存；搜索与目录请求互不覆盖。 */
export function useFileTree(
  root: ProjectTreePage | null,
  load: (id: string, offset: number) => Promise<ProjectTreePage>,
  search: (query: string, offset: number) => Promise<ProjectTreePage>,
) {
  const current = useRef({ root, load, search });
  current.current = { root, load, search };
  const [query, setQuery] = useState("");
  const queryRef = useRef(query);
  queryRef.current = query;
  const [pages, setPages] = useState<Record<string, ProjectTreePage>>({});
  const [opened, setOpened] = useState<ReadonlySet<string>>(new Set());
  const [searchPage, setSearchPage] = useState<{
    query: string;
    page: ProjectTreePage;
  } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [searching, setSearching] = useState(false);
  const [busy, setBusy] = useState<ReadonlySet<string>>(new Set());
  const pending = useRef(new Set<string>());
  const searchSequence = useRef(0);
  useEffect(() => {
    setPages(root ? { [root.directoryId]: root } : {});
    setOpened(new Set());
    setSearchPage(null);
    setSearching(false);
    setError(null);
    setBusy(new Set());
    pending.current.clear();
    return () => {
      searchSequence.current++;
    };
  }, [root]);
  /** 读取一页并保留先前页；迟到树响应不进入当前视图。 */
  async function loadPage(id: string, offset: number): Promise<void> {
    const treeId = current.current.root?.treeId;
    if (!treeId || pending.current.has(id)) return;
    pending.current.add(id);
    setBusy(new Set(pending.current));
    setError(null);
    try {
      const page = await current.current.load(id, offset);
      if (current.current.root?.treeId !== treeId) return;
      setPages((old) => ({
        ...old,
        [id]: {
          ...page,
          entries: offset
            ? [...(old[id]?.entries ?? []), ...page.entries]
            : page.entries,
        },
      }));
    } catch (cause: unknown) {
      if (current.current.root?.treeId === treeId)
        setError(describeGitError(normalizeOperationError(cause)));
    } finally {
      if (current.current.root?.treeId === treeId) {
        pending.current.delete(id);
        setBusy(new Set(pending.current));
      }
    }
  }
  /** 搜索采用独立请求代次，旧关键词或旧页不能覆盖新搜索。 */
  async function find(offset: number): Promise<void> {
    const text = queryRef.current.trim();
    const treeId = current.current.root?.treeId;
    if (!treeId || !text) return;
    const sequence = ++searchSequence.current;
    setSearching(true);
    setError(null);
    try {
      const page = await current.current.search(text, offset);
      if (
        sequence !== searchSequence.current ||
        current.current.root?.treeId !== treeId ||
        queryRef.current.trim() !== text
      )
        return;
      setSearchPage((old) => ({
        query: text,
        page: {
          ...page,
          entries:
            offset && old?.query === text
              ? [...old.page.entries, ...page.entries]
              : page.entries,
        },
      }));
    } catch (cause: unknown) {
      if (
        sequence === searchSequence.current &&
        current.current.root?.treeId === treeId
      )
        setError(describeGitError(normalizeOperationError(cause)));
    } finally {
      if (sequence === searchSequence.current) setSearching(false);
    }
  }
  useEffect(() => {
    searchSequence.current++;
    setSearchPage(null);
    setSearching(Boolean(query.trim() && root));
    if (!query.trim() || !root) return;
    const timer = window.setTimeout(() => {
      void find(0);
    }, 200);
    return () => {
      window.clearTimeout(timer);
      searchSequence.current++;
    };
  }, [query, root]);
  const directoryIds = useMemo(() => {
    const ids = new Map<string, string>();
    if (root) ids.set("", root.directoryId);
    for (const page of Object.values(pages))
      for (const entry of page.entries)
        if (entry.kind === "directory") ids.set(entry.path, entry.id);
    return ids;
  }, [pages, root]);
  const visibleSearch =
    searchPage?.query === query.trim() &&
    searchPage.page.treeId === root?.treeId
      ? searchPage.page
      : null;
  const nodes = useMemo(() => {
    /** 只展开已加载且当前打开的目录，不预构造所有后代节点。 */
    function children(
      page: ProjectTreePage | null | undefined,
    ): FileTreeNode[] {
      return (
        page?.entries.map((entry) =>
          entry.kind === "file"
            ? {
                kind: "file",
                name: query.trim() ? entry.path : entry.name,
                path: entry.path,
                fileId: entry.id,
                status: entry.status,
              }
            : {
                kind: "directory",
                name: entry.name,
                path: entry.path,
                children: opened.has(entry.path)
                  ? children(pages[entry.id])
                  : [],
              },
        ) ?? []
      );
    }
    return children(
      query.trim()
        ? visibleSearch
        : root
          ? (pages[root.directoryId] ?? root)
          : null,
    );
  }, [pages, root, query, visibleSearch, opened]);
  /** 展开时首次获取直接子项，再次展开复用缓存。 */
  function toggle(path: string): () => void {
    return () => {
      const id = directoryIds.get(path);
      if (!id) return;
      setOpened((old) => {
        const next = new Set(old);
        if (next.has(path)) next.delete(path);
        else next.add(path);
        return next;
      });
      if (!pages[id]) void loadPage(id, 0);
    };
  }
  /** 选择更多只继续当前目录或当前搜索，不重建文件会话。 */
  function more(path: string): () => void {
    return () => {
      if (query.trim()) {
        if (visibleSearch?.nextOffset != null)
          void find(visibleSearch.nextOffset);
        return;
      }
      const id = directoryIds.get(path);
      const offset = id ? pages[id]?.nextOffset : null;
      if (id && offset != null) void loadPage(id, offset);
    };
  }
  /** 分页状态对应当前可见目录。 */
  function hasMore(path: string): boolean {
    const id = directoryIds.get(path);
    return query.trim()
      ? visibleSearch?.nextOffset != null
      : Boolean(id && pages[id]?.nextOffset != null);
  }
  /** 目录加载状态用于避免重复请求并向用户说明等待。 */
  function loading(path: string): boolean {
    return query.trim() ? searching : busy.has(directoryIds.get(path) ?? "");
  }
  /** 当前展开状态供按钮与读屏使用。 */
  function isOpen(path: string): boolean {
    return opened.has(path);
  }
  /** 查询长度与后端限制一致，不修改目录折叠状态。 */
  function filter(event: ChangeEvent<HTMLInputElement>): void {
    setQuery(event.target.value);
  }
  return {
    nodes,
    query,
    filter,
    isOpen,
    toggle,
    more,
    hasMore,
    loading,
    error,
  };
}
