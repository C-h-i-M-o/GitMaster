import { useEffect, useRef, useState } from "react";
import {
  readWorkspaceSessions,
  writeWorkspaceSession,
} from "../services/workspaceSession";
import type {
  WorkspaceSession,
  DocumentPosition,
} from "../types/workspaceSession";
import type { RepositoryState } from "../types/git";
import { workspaceDocumentPaths, workspacePathKey } from "../ui/workspacePath";

interface SessionOptions {
  enabled: boolean;
  ready: boolean;
  repository: RepositoryState | null;
  paths: readonly string[];
  activePath: string | null;
  page: WorkspaceSession["activePage"];
  open(path: string): Promise<void>;
  restore(session: WorkspaceSession, current: () => boolean): Promise<void>;
}
const emptyPaths: readonly string[] = [];

/** 只持久化工作区位置；恢复使用新能力，手动打开优先于启动恢复。 */
export function useWorkspaceSession(options: SessionOptions) {
  const current = useRef(options);
  current.current = options;
  const sessions = useRef<WorkspaceSession[]>([]);
  const lastRoot = useRef<string | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [expandedPaths, setExpandedPaths] =
    useState<readonly string[]>(emptyPaths);
  const expanded = useRef<readonly string[]>(emptyPaths);
  const positions = useRef(new Map<string, DocumentPosition>());
  const manual = useRef(false);
  const startup = useRef(false);
  const generation = useRef(0);
  const initialized = useRef<string | null>(null);
  const restored = useRef<string | null>(null);
  const pending = useRef<WorkspaceSession | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const writes = useRef<Promise<void>>(Promise.resolve());
  const [revision, setRevision] = useState(0);

  /** 串行保存最后一次视图快照，错误只影响恢复提示。 */
  function flush(): Promise<void> {
    if (timer.current) clearTimeout(timer.current);
    timer.current = null;
    const session = pending.current;
    pending.current = null;
    if (session) {
      sessions.current = [
        session,
        ...sessions.current.filter(
          (item) =>
            workspacePathKey(item.rootPath) !==
            workspacePathKey(session.rootPath),
        ),
      ].slice(0, 20);
      writes.current = writes.current
        .then(() => writeWorkspaceSession(session))
        .catch(() => {
          setNotice("工作区位置保存失败，文档编辑不受影响。");
        });
    }
    return writes.current;
  }
  /** 当前位置变化合并写入，不持久化草稿或文件能力。 */
  function capture(): void {
    const value = current.current;
    if (
      !value.enabled ||
      !value.repository ||
      restored.current !== value.repository.repositoryId
    )
      return;
    const paths = workspaceDocumentPaths(value.paths, value.activePath);
    pending.current = {
      rootPath: value.repository.rootPath,
      expandedPaths: [...expanded.current].slice(0, 200),
      documents: paths.map((path) => ({
        path,
        position: positions.current.get(path) ?? null,
      })),
      activePath:
        value.activePath && paths.includes(value.activePath)
          ? value.activePath
          : null,
      activePage: value.page,
    };
    if (timer.current) clearTimeout(timer.current);
    timer.current = setTimeout(() => {
      void flush();
    }, 250);
  }
  useEffect(() => {
    if (!options.enabled) return;
    let disposed = false;
    void readWorkspaceSessions()
      .then((stored) => {
        if (disposed) return;
        sessions.current = stored.projects;
        lastRoot.current = stored.lastRootPath;
        setLoaded(true);
      })
      .catch(() => {
        if (!disposed) {
          setNotice("工作区位置记录无法读取，可从最近项目重新打开。");
          setLoaded(true);
        }
      });
    return () => {
      disposed = true;
    };
  }, [options.enabled]);
  useEffect(() => {
    if (!loaded || !options.ready || startup.current) return;
    startup.current = true;
    if (!manual.current && !options.repository && lastRoot.current)
      void current.current.open(lastRoot.current);
  }, [loaded, options.ready, options.repository]);
  useEffect(() => {
    if (!loaded) return;
    const repository = options.repository;
    if (!repository || initialized.current === repository.repositoryId) return;
    void flush();
    initialized.current = repository.repositoryId;
    restored.current = null;
    const token = ++generation.current;
    const session = sessions.current.find(
      (item) =>
        workspacePathKey(item.rootPath) ===
        workspacePathKey(repository.rootPath),
    );
    expanded.current = session?.expandedPaths ?? emptyPaths;
    positions.current = new Map(
      session?.documents.flatMap((item) =>
        item.position ? [[item.path, item.position] as const] : [],
      ) ?? [],
    );
    setExpandedPaths(expanded.current);
    /** 仓库切换或用户新打开请求会使迟到的恢复失效。 */
    const valid = (): boolean =>
      generation.current === token &&
      current.current.repository?.repositoryId === repository.repositoryId;
    void (async () => {
      try {
        if (session) await current.current.restore(session, valid);
      } catch {
        if (valid())
          setNotice("部分上次打开的文件已不可用，其余工作区已保留。");
      } finally {
        if (valid()) {
          restored.current = repository.repositoryId;
          setRevision((value) => value + 1);
        }
      }
    })();
  }, [loaded, options.repository?.repositoryId]);
  useEffect(() => {
    capture();
  }, [
    options.repository,
    options.paths.join("\0"),
    options.activePath,
    options.page,
    revision,
  ]);
  useEffect(
    () => () => {
      generation.current++;
      void flush();
    },
    [],
  );
  /** 用户主动选项目后不再允许启动恢复接管界面。 */
  function userOpening(): void {
    manual.current = true;
    generation.current++;
    restored.current = current.current.repository?.repositoryId ?? null;
    void flush();
  }
  /** 展开变化仅更新当前项目的路径状态，不改变恢复种子。 */
  function recordExpanded(paths: string[]): void {
    expanded.current = paths;
    capture();
  }
  /** 编辑器滚动与光标变化共用有界位置记录。 */
  function recordPosition(path: string, position: DocumentPosition): void {
    positions.current.set(path, position);
    capture();
  }
  /** 新模型首次激活时恢复记录，已有模型仍使用 Monaco 原生视图状态。 */
  function readPosition(path: string): DocumentPosition | null {
    return positions.current.get(path) ?? null;
  }
  return {
    expandedPaths,
    recordExpanded,
    recordPosition,
    readPosition,
    flush,
    userOpening,
    notice,
  };
}
