import { useEffect, useRef, useState, type ChangeEvent } from "react";
import { agentApi, agentError } from "../services/agent";
import type { ChatSearchResult } from "../types/agentMemory";

interface SearchState {
  key: string;
  results: ChatSearchResult[];
  unavailable: string[];
  busy: boolean;
  error: string | null;
}

/** 按工作区和归档范围搜索聊天，并丢弃过期请求结果。 */
export function useAgentSearch(
  enabled: boolean,
  workspaceId: string,
  archived: boolean,
) {
  const [query, setQuery] = useState("");
  const [state, setState] = useState<SearchState>({
    key: "",
    results: [],
    unavailable: [],
    busy: false,
    error: null,
  });
  const generation = useRef(0);
  const key = JSON.stringify([workspaceId, archived, query]);

  /** 将搜索字数限制在后端接受的 200 个 Unicode 字符。 */
  function change(event: ChangeEvent<HTMLInputElement>): void {
    setQuery(Array.from(event.target.value).slice(0, 200).join(""));
  }

  useEffect(() => {
    const request = ++generation.current;
    const normalized = query.trim();
    if (!enabled || !normalized) {
      setState({ key, results: [], unavailable: [], busy: false, error: null });
      return () => {
        generation.current += 1;
      };
    }
    setState({ key, results: [], unavailable: [], busy: true, error: null });
    const timer = window.setTimeout(() => {
      void agentApi
        .searchChats(workspaceId, normalized, archived)
        .then((response) => {
          if (generation.current === request)
            setState({
              key,
              results: response.results.slice(0, 100),
              unavailable: response.unavailable,
              busy: false,
              error: null,
            });
        })
        .catch((cause: unknown) => {
          if (generation.current === request)
            setState({
              key,
              results: [],
              unavailable: [],
              busy: false,
              error: agentError(cause),
            });
        });
    }, 250);
    return () => {
      window.clearTimeout(timer);
      generation.current += 1;
    };
  }, [enabled, workspaceId, archived, query, key]);

  const current = state.key === key;
  return {
    query,
    change,
    results: current ? state.results : [],
    unavailable: current ? state.unavailable : [],
    busy: current ? state.busy : enabled && query.trim().length > 0,
    error: current ? state.error : null,
  };
}
