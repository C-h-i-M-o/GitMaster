import {
  useCallback,
  useEffect,
  useState,
  type ChangeEvent,
  type FormEvent,
} from "react";
import { agentApi, agentError } from "../services/agent";
import type {
  AgentSettingsView,
  DeviceUsage,
  PermissionMode,
  ProviderProfile,
  ProviderProtocol,
  SaveProfile,
} from "../types/agent";
/** 创建未持久化配置草稿；API Key 始终只存在于当前表单内存。 */
function emptyProfile(): SaveProfile {
  return {
    id: null,
    name: "",
    protocol: "chatCompletions",
    baseUrl: "https://api.openai.com/v1",
    model: "",
    apiKey: "",
  };
}
/** 三种协议使用各自官方默认根地址，用户可改为兼容服务。 */
function baseUrl(protocol: ProviderProtocol): string {
  return protocol === "anthropicMessages"
    ? "https://api.anthropic.com/v1"
    : "https://api.openai.com/v1";
}
/** 管理 API 配置、权限、首配和此设备统计，所有写入均经桌面后端。 */
export function useAgentSettings(enabled: boolean) {
  const [view, setView] = useState<AgentSettingsView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [deletePending, setDeletePending] = useState<ProviderProfile | null>(
    null,
  );
  const [setupOpen, setSetupOpen] = useState(false);
  const [draft, setDraft] = useState<SaveProfile>(emptyProfile);
  const [usage, setUsage] = useState<DeviceUsage | null>(null);
  const [usageError, setUsageError] = useState<string | null>(null);
  const [usageBusy, setUsageBusy] = useState(false);
  /** 初始化仅一次；用户关闭首配后普通 Git 仍然可用。 */
  useEffect(() => {
    if (!enabled) return;
    let live = true;
    void agentApi
      .settings()
      .then((result) => {
        if (live) {
          setView(result);
          setSetupOpen(result.settings.profiles.length === 0);
        }
      })
      .catch((cause: unknown) => {
        if (live) setError(agentError(cause));
      });
    return () => {
      live = false;
    };
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
  /** 只允许修改表单定义的字段。 */
  function field(name: "name" | "baseUrl" | "model" | "apiKey") {
    return (event: ChangeEvent<HTMLInputElement>) =>
      setDraft((current) => ({ ...current, [name]: event.target.value }));
  }
  /** 协议切换仅替换官方默认地址，不覆盖用户的兼容服务地址。 */
  function protocolChanged(event: ChangeEvent<HTMLSelectElement>) {
    const protocol = event.target.value;
    if (
      protocol !== "chatCompletions" &&
      protocol !== "responses" &&
      protocol !== "anthropicMessages"
    )
      return;
    setDraft((current) => ({
      ...current,
      protocol,
      baseUrl:
        current.baseUrl === baseUrl(current.protocol)
          ? baseUrl(protocol)
          : current.baseUrl,
    }));
  }
  /** 新增配置，密码输入默认空白。 */
  function add() {
    setDraft(emptyProfile());
    setError(null);
    setNotice(null);
    setSetupOpen(true);
  }
  /** 编辑只回填普通字段，不读取已保存密钥。 */
  function edit(profile: ProviderProfile) {
    return () => {
      setDraft({
        id: profile.id,
        name: profile.name,
        protocol: profile.protocol,
        baseUrl: profile.baseUrl,
        model: profile.model,
        apiKey: "",
      });
      setError(null);
      setNotice(null);
      setSetupOpen(true);
    };
  }
  /** 关闭清除密码，失败后也不把密码保留在后台页面。 */
  function closeSetup() {
    if (busy) return;
    setDraft(emptyProfile());
    setSetupOpen(false);
  }
  /** 保存后清除密码；不在浏览器缓存、日志或 URL 中留下草稿。 */
  async function save(event: FormEvent) {
    event.preventDefault();
    const success = await change(() => agentApi.save(draft));
    setDraft((current) => ({ ...current, apiKey: "" }));
    if (success) {
      setSetupOpen(false);
      setNotice("API 配置已保存，密钥由系统密钥库管理。");
    }
  }
  /** 打开明确的删除确认，聊天记录仍保留。 */
  function remove(profile: ProviderProfile) {
    return () => setDeletePending(profile);
  }
  /** 取消删除不更改系统密钥库。 */
  function keepProfile() {
    setDeletePending(null);
  }
  /** 只删除确认中绑定的配置和凭据。 */
  async function confirmDelete() {
    if (
      deletePending &&
      (await change(() => agentApi.deleteProfile(deletePending.id)))
    )
      setDeletePending(null);
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
  /** 明确由用户触发连接测试，可能消耗一次模型调用。 */
  function test(profile: ProviderProfile) {
    return async () => {
      if (busy) return;
      setBusy(true);
      setError(null);
      setNotice(null);
      try {
        await agentApi.test(profile.id);
        setNotice(`“${profile.name}”连接成功。`);
      } catch (cause) {
        setError(agentError(cause));
      } finally {
        setBusy(false);
      }
    };
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
    deletePending,
    keepProfile,
    confirmDelete,
    view,
    error,
    notice,
    busy,
    setupOpen,
    draft,
    usage,
    usageError,
    usageBusy,
    refreshUsage,
    field,
    protocolChanged,
    add,
    edit,
    closeSetup,
    save,
    remove,
    select,
    permission,
    test,
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
    label: "请求批准",
    description:
      "项目内普通读写可自动执行；项目外文件、联网及风险 Git 操作询问。",
  },
  {
    value: "autoApprove",
    label: "帮我批准",
    description:
      "低风险项目外读写和公开联网自动执行；敏感路径、私网及风险 Git 操作询问。",
  },
  {
    value: "fullAccess",
    label: "完全访问权限",
    description:
      "注册工具可访问任意文件和网络，不逐次询问；仍受系统权限、文件版本和工具范围约束。",
  },
];
