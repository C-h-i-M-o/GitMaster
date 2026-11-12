import { useCallback, useEffect, useState, type ChangeEvent } from "react";
import { isTauri } from "@tauri-apps/api/core";
import type {
  AiModel,
  AiProvider,
  AiProtocol,
  AiSettingsSnapshot,
} from "../types/ai";
import * as api from "../services/ai";

/** 每次编辑都独立持有草稿；已保存密钥不会回填。 */
function blankProvider(): AiProvider {
  return {
    providerId: "",
    displayName: "",
    protocol: "openai-completions",
    baseUrl: "",
    credentialRef: null,
    allowInsecureHttp: false,
    models: [
      { modelId: "", displayName: "", contextWindow: 0, maxOutputTokens: 0 },
    ],
  };
}
/** 新模型没有猜测容量，必须由用户明确填写。 */
function blankModel(): AiModel {
  return { modelId: "", displayName: "", contextWindow: 0, maxOutputTokens: 0 };
}
/** 模型设置的读取、表单事件和保存逻辑集中在独立 hook。 */
export function useAiModels() {
  const [snapshot, setSnapshot] = useState<AiSettingsSnapshot | null>(null);
  const [draft, setDraft] = useState<AiProvider | null>(null);
  const [editing, setEditing] = useState(false);
  const [key, setKey] = useState("");
  const [removeKey, setRemoveKey] = useState(false);
  const [showKey, setShowKey] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  /** 外部保存通知只重新读取脱敏数据，不触发自动保存循环。 */
  const reload = useCallback(async (): Promise<void> => {
    if (!isTauri()) return;
    try {
      setSnapshot(await api.readAiSettings());
      setError(null);
    } catch (cause) {
      setError(api.aiError(cause));
    }
  }, []);
  useEffect(() => {
    void reload();
    const listener = (): void => {
      void reload();
    };
    window.addEventListener("gitmaster:ai-settings", listener);
    return () => window.removeEventListener("gitmaster:ai-settings", listener);
  }, [reload]);
  /** 创建提供商前清空旧密钥输入。 */
  function add(): void {
    setDraft(blankProvider());
    setEditing(false);
    setKey("");
    setRemoveKey(false);
    setError(null);
  }
  /** 编辑从已保存的脱敏副本开始。 */
  function edit(provider: AiProvider): () => void {
    return () => {
      setDraft(structuredClone(provider));
      setEditing(true);
      setKey("");
      setRemoveKey(false);
      setError(null);
    };
  }
  /** 放弃表单仅释放内存中的密钥草稿。 */
  function cancel(): void {
    setDraft(null);
    setKey("");
    setRemoveKey(false);
    setShowKey(false);
  }
  /** 字符串字段保持显式类型，不将协议外值强制转换。 */
  function field(
    name: "providerId" | "displayName" | "baseUrl",
  ): (event: ChangeEvent<HTMLInputElement>) => void {
    return (event) =>
      setDraft((old) => (old ? { ...old, [name]: event.target.value } : old));
  }
  /** 仅接受三种协议选项。 */
  function protocol(event: ChangeEvent<HTMLSelectElement>): void {
    const value = event.target.value;
    if (
      ["openai-completions", "openai-responses", "anthropic-messages"].includes(
        value,
      )
    )
      setDraft((old) =>
        old ? { ...old, protocol: value as AiProtocol } : old,
      );
  }
  /** 切换密钥输入总是恢复遮蔽状态。 */
  function changeKey(event: ChangeEvent<HTMLInputElement>): void {
    setKey(event.target.value);
    setShowKey(false);
    setRemoveKey(false);
  }
  /** 删除密钥必须是显式表单动作，并在保存时生效。 */
  function clearKey(): void {
    setKey("");
    setRemoveKey(true);
  }
  /** 只显示用户本次键入的密钥，不读取系统中的旧密钥。 */
  function toggleKey(): void {
    setShowKey((old) => !old);
  }
  /** 远程明文连接需要单独勾选确认。 */
  function insecure(event: ChangeEvent<HTMLInputElement>): void {
    setDraft((old) =>
      old ? { ...old, allowInsecureHttp: event.target.checked } : old,
    );
  }
  /** 每个模型的可编辑输入显式区分文字和数值。 */
  function modelField(
    index: number,
    name: "modelId" | "displayName" | "contextWindow" | "maxOutputTokens",
  ): (event: ChangeEvent<HTMLInputElement>) => void {
    return (event) => {
      const value =
        name === "contextWindow" || name === "maxOutputTokens"
          ? Number(event.target.value)
          : event.target.value;
      setDraft((old) =>
        old
          ? {
              ...old,
              models: old.models.map((model, row) =>
                row === index ? { ...model, [name]: value } : model,
              ),
            }
          : old,
      );
    };
  }
  /** 新增模型只添加当前表单所需的一行。 */
  function addModel(): void {
    setDraft((old) =>
      old ? { ...old, models: [...old.models, blankModel()] } : old,
    );
  }
  /** 至少一个模型的约束仍由后端再次校验。 */
  function removeModel(index: number): () => void {
    return () =>
      setDraft((old) =>
        old
          ? { ...old, models: old.models.filter((_, row) => row !== index) }
          : old,
      );
  }
  /** 成功落盘后才广播变更，失败保留全部输入以便修正。 */
  async function save(): Promise<void> {
    if (!snapshot || !draft || busy) return;
    setBusy(true);
    setError(null);
    try {
      const provider = {
        ...draft,
        providerId: draft.providerId.trim(),
        baseUrl: draft.baseUrl.trim(),
        displayName: draft.displayName.trim() || draft.providerId.trim(),
        models: draft.models.map((model) => ({
          ...model,
          modelId: model.modelId.trim(),
          displayName: model.displayName.trim() || model.modelId.trim(),
        })),
      };
      const result = await api.saveAiProvider(
        snapshot.settings.revision,
        provider,
        removeKey
          ? { kind: "remove" }
          : key.trim()
            ? { kind: "replace", value: key }
            : { kind: "keep" },
      );
      setSnapshot(result);
      setNotice(result.warning ?? "配置已保存。");
      cancel();
      window.dispatchEvent(new Event("gitmaster:ai-settings"));
    } catch (cause) {
      setError(api.aiError(cause));
    } finally {
      setBusy(false);
    }
  }
  /** 删除提供商需要明确点击确认，已删除模型不会自动替换。 */
  function remove(provider: AiProvider): () => void {
    return () => {
      if (
        !snapshot ||
        !window.confirm(
          `删除模型提供商“${provider.displayName}”？历史会话会保留。`,
        )
      )
        return;
      setBusy(true);
      void api
        .deleteAiProvider(snapshot.settings.revision, provider.providerId)
        .then((value) => {
          setSnapshot(value);
          setNotice(value.warning ?? "提供商已删除。");
          window.dispatchEvent(new Event("gitmaster:ai-settings"));
        })
        .catch((cause: unknown) => setError(api.aiError(cause)))
        .finally(() => setBusy(false));
    };
  }
  /** 连接测试只检查生成路由，不替代带工具的协议验收。 */
  function testConnection(provider: AiProvider): () => void {
    return () => {
      const model = provider.models[0];
      if (!model) return;
      setBusy(true);
      setNotice(null);
      setError(null);
      void api
        .testAiProvider(provider.providerId, model.modelId)
        .then((result) => {
          if (result.ok)
            setNotice(
              "连接成功（已收到文本或推理响应）。完整回答和工具能力以会话实际运行结果为准。",
            );
          else setError(result.error.message);
        })
        .catch((cause: unknown) => setError(api.aiError(cause)))
        .finally(() => setBusy(false));
    };
  }
  return {
    snapshot,
    draft,
    editing,
    key,
    removeKey,
    showKey,
    busy,
    error,
    notice,
    testConnection,
    desktop: isTauri(),
    reload,
    add,
    edit,
    cancel,
    field,
    protocol,
    changeKey,
    clearKey,
    toggleKey,
    insecure,
    modelField,
    addModel,
    removeModel,
    save,
    remove,
  };
}
