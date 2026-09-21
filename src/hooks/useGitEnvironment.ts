import { useCallback, useEffect, useRef, useState } from "react";
import { detectGit, setGitPath, isDesktop } from "../services/git";
import { normalizeOperationError } from "../services/gitErrors";
import type { GitEnvironment, OperationError } from "../types/git";
export interface GitEnvironmentState {
  status: "preview" | "loading" | "ready" | "error";
  environment: GitEnvironment | null;
  error: OperationError | null;
}
/** 管理环境请求代次，失败的手动设置保留最后成功的 Git。 */
export function useGitEnvironment() {
  const [state, setState] = useState<GitEnvironmentState>({
    status: isDesktop() ? "loading" : "preview",
    environment: null,
    error: null,
  });
  const generation = useRef(0);
  const active = useRef(true);
  /** 执行一次环境请求；用户切换设置后迟到检测不覆盖新结果。 */
  const run = useCallback(
    async (
      task: () => Promise<GitEnvironment>,
      preserve: boolean,
    ): Promise<boolean> => {
      if (!isDesktop()) return false;
      const token = ++generation.current;
      setState((old) => ({ ...old, status: "loading", error: null }));
      try {
        const environment = await task();
        if (!active.current || token !== generation.current) return false;
        setState({
          status: "ready",
          environment,
          error:
            environment.status === "unavailable" ? environment.error : null,
        });
        return true;
      } catch (error: unknown) {
        if (active.current && token === generation.current)
          setState((old) => ({
            status: "error",
            environment: preserve ? old.environment : null,
            error: normalizeOperationError(error),
          }));
        return false;
      }
    },
    [],
  );
  /** 根据当前保存设置检测环境。 */
  const refresh = useCallback(() => run(detectGit, false), [run]);
  /** 保存已选择路径或显式恢复自动检测。 */
  const choose = useCallback(
    (path: string | null) => run(() => setGitPath(path), true),
    [run],
  );
  useEffect(() => {
    active.current = true;
    void refresh();
    return () => {
      active.current = false;
      generation.current += 1;
    };
  }, [refresh]);
  return { ...state, refresh, choose };
}
