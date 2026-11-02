import { useEffect } from "react";
import type { AgentSettingsController } from "./useAgentSettings";
/** 设置分区出现时才扫描磁盘，不在聊天流式更新时重复读取。 */
export function useDeviceStorage(s: AgentSettingsController) {
  useEffect(() => {
    void s.refreshUsage();
  }, [s.refreshUsage]);
}
