import { useCallback, useEffect, useState, type ChangeEvent } from "react";
import { agentApi, agentError } from "../services/agent";
import { useAgentConfiguration } from "./useAgentConfiguration";
import type {
  AgentSettingsView,
  DeviceUsage,
  PermissionMode,
} from "../types/agent";
/** 管理兼容聊天设置入口、模型配置流程、权限和此设备统计。 */
export function useAgentSettings(enabled: boolean) {
  const [view, setView] = useState<AgentSettingsView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [usage, setUsage] = useState<DeviceUsage | null>(null);
  const [usageError, setUsageError] = useState<string | null>(null);
  const [usageBusy, setUsageBusy] = useState(false);
  /** 读取设置只更新状态；AI 首次引导由主界面按需呈现。 */
  useEffect(() => {
    if (!enabled) return;
    let live = true;
    void agentApi
      .settings()
      .then((result) => {
        if (live) {
          setView(result);
        }
      })
      .catch((cause: unknown) => {
        if (live) setError(agentError(cause));
      });
    return () => {
      live = false;
    };
  }, [enabled]);
  const configuration = useAgentConfiguration(
    enabled,
    view,
    setView,
    setError,
    setNotice,
  );
  /** 完成对话后重读模型列表与最近使用记录。 */
  const refreshSettings = useCallback(async () => {
    if (!enabled) return;
    try {
      setView(await agentApi.settings());
    } catch (cause) {
      setError(agentError(cause));
    }
  }, [enabled]);
  /** 设置操作统一保留失败提示和旧快照。 */
  async function change(
    task: () => Promise<AgentSettingsView>,
  ): Promise<boolean> {
    if (busy || !enabled) return false;
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      setView(await task());
      return true;
    } catch (cause) {
      setError(agentError(cause));
      return false;
    } finally {
      setBusy(false);
    }
  }
  /** 保留聊天入口的兼容控制器并打开新版配置流程。 */
  function add(): void {
    setError(null);
    setNotice(null);
    configuration.add();
  }
  /** 后续运行使用选中的配置，当前运行不变。 */
  function select(event: ChangeEvent<HTMLSelectElement>) {
    const id = event.target.value;
    if (id) void change(() => agentApi.select(id));
  }
  /** 三种权限只接受后端定义的枚举。 */
  function permission(event: ChangeEvent<HTMLSelectElement>) {
    const mode = event.target.value;
    if (
      mode === "requestApproval" ||
      mode === "autoApprove" ||
      mode === "fullAccess"
    )
      void change(() => agentApi.permission(mode));
  }
  /** 每次打开或刷新时读取实际磁盘占用，失败保留错误而非伪造零。 */
  const refreshUsage = useCallback(async () => {
    if (!enabled) return;
    setUsageBusy(true);
    setUsageError(null);
    try {
      setUsage(await agentApi.usage());
    } catch (cause) {
      setUsageError(agentError(cause));
    } finally {
      setUsageBusy(false);
    }
  }, [enabled]);
  return {
    enabled,
    view,
    error,
    notice,
    busy: busy || configuration.busy,
    setupOpen: configuration.setupOpen,
    configuration,
    usage,
    usageError,
    usageBusy,
    refreshUsage,
    refreshSettings,
    add,
    select,
    permission,
  };
}
export type AgentSettingsController = ReturnType<typeof useAgentSettings>;
export const permissionOptions: {
  value: PermissionMode;
  label: string;
  description: string;
}[] = [
  {
    value: "requestApproval",
    label: "逐次审批",
    description:
      "工作区普通文件与 Git 读取自动；文件和 Git 写入、网络访问均需审批。",
  },
  {
    value: "autoApprove",
    label: "工作区协作",
    description:
      "工作区普通文件写入、本地暂存和提交可自动；联网、分支切换、push/pull 等风险操作仍需审批。",
  },
  {
    value: "fullAccess",
    label: "自主执行",
    description:
      "当前已启用工具按“工作区协作”的规则执行；工作区边界、敏感及程序文件保护仍有效，沙箱命令尚未启用。",
  },
];
