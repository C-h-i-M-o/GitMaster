import { useCallback, useEffect, useRef, useState } from "react";
import { readRecentProjects } from "../services/recentProjects";
import type { RecentProject } from "../types/recentProjects";

/** 加载并刷新后端最近项目；读取失败只影响最近项目区域。 */
export function useRecentProjects(): {
  projects: RecentProject[];
  refresh: () => void;
  error: unknown;
} {
  const [projects, setProjects] = useState<RecentProject[]>([]);
  const [error, setError] = useState<unknown>(null);
  const request = useRef(0);
  const refresh = useCallback((): void => {
    const token = ++request.current;
    void readRecentProjects()
      .then((value) => {
        if (token !== request.current) return;
        setProjects(value);
        setError(null);
      })
      .catch((cause: unknown) => {
        if (token === request.current) setError(cause);
      });
  }, []);
  useEffect(() => {
    refresh();
    return () => {
      request.current += 1;
    };
  }, [refresh]);
  return { projects, refresh, error };
}
