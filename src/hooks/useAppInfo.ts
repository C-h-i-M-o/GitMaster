import { useEffect, useState } from "react";
import { readAppInfo } from "../services/desktop";
import type { AppInfoState } from "../types/app";

/** 管理应用信息请求，卸载后不应用迟到的结果。 */
export function useAppInfo(): AppInfoState {
  const [state, setState] = useState<AppInfoState>({ status: "loading" });

  // 仅将当前组件生命周期内的响应同步到 UI。
  useEffect(() => {
    let active = true;

    void readAppInfo().then((nextState) => {
      if (active) {
        setState(nextState);
      }
    });

    // 防止组件卸载或严格模式重挂载时使用过期响应。
    return () => {
      active = false;
    };
  }, []);

  return state;
}
