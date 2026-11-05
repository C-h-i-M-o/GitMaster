import {
  useEffect,
  useRef,
  useState,
  type ChangeEvent,
  type Dispatch,
  type FormEvent,
  type SetStateAction,
} from "react";
import { agentApi, agentError } from "../services/agent";
import type {
  AgentSettingsView,
  ModelConfig,
  ProviderConnection,
} from "../types/agent";
import type {
  ConnectionTestReport,
  DiscoveredModel,
  ModelDraft,
  ModelParameters,
  ProviderDraft,
  ProviderPreset,
  PresetModel,
  SaveModel,
} from "../types/agentModels";
import {
  capabilitiesMatchEndpoint,
  configurationRequestId,
  isCurrentConfigurationResponse,
  modelDraftForDiscovered,
  modelDraftForEdit,
  modelDraftForManual,
  modelDraftForPreset,
  modelParameterAvailability,
  modelParameterErrors,
  providerDraftForEdit,
  providerDraftForPreset,
  providerHostChanged,
  providerKeyError,
  saveProviderRequest,
  unknownModelCapabilities,
  unknownModelParameters,
  type ModelParameterErrors,
  type ModelParameterField,
} from "../ui/agentModelConfiguration";

type ConfigurationMode = "provider" | "model" | null;
type ProviderField = "name" | "baseUrl" | "apiKey";
type ModelField = "modelId" | "label" | "protocol";
type ConfigurationRequest = { id: string; revision: number };
let requestSequence = 0;

/** 生成本地配置请求 ID，仅用于取消与丢弃迟到响应。 */
function newRequestId(): string {
  requestSequence += 1;
  return configurationRequestId(Date.now(), requestSequence);
}

/** 将空数字输入保留为供应商默认，其余输入转成有限数值。 */
function numericValue(input: HTMLInputElement): number | null {
  if (input.value.trim() === "") return null;
  const value = input.valueAsNumber;
  return Number.isFinite(value) ? value : null;
}

/** 编辑供应商、模型、目录和连接测试的内存草稿及取消代次。 */
export function useAgentConfiguration(
  enabled: boolean,
  view: AgentSettingsView | null,
  setView: Dispatch<SetStateAction<AgentSettingsView | null>>,
  setError: Dispatch<SetStateAction<string | null>>,
  setNotice: Dispatch<SetStateAction<string | null>>,
) {
  const [mode, setMode] = useState<ConfigurationMode>(null);
  const [provider, setProvider] = useState<ProviderDraft>({
    id: null,
    presetId: null,
    name: "",
    baseUrl: "",
    protocol: "chatCompletions",
    apiKey: null,
    allowStoredKeyForChangedHost: false,
  });
  const [models, setModels] = useState<ModelDraft[]>([]);
  const [modelDraft, setModelDraft] = useState<ModelDraft | null>(null);
  const [selectedModelId, setSelectedModelId] = useState<string | null>(null);
  const [providerRevision, setProviderRevision] = useState<number | null>(null);
  const [originalBaseUrl, setOriginalBaseUrl] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [localError, setLocalError] = useState<string | null>(null);
  const [parameterErrors, setParameterErrors] = useState<ModelParameterErrors>(
    {},
  );
  const [manualModelId, setManualModelId] = useState("");
  const [search, setSearch] = useState("");
  const [discoveryModels, setDiscoveryModels] = useState<DiscoveredModel[]>([]);
  const [discoveryRequestId, setDiscoveryRequestId] = useState<string | null>(
    null,
  );
  const [discoveryCursor, setDiscoveryCursor] = useState<string | null>(null);
  const [discoveryBusy, setDiscoveryBusy] = useState(false);
  const [discoveryError, setDiscoveryError] = useState<string | null>(null);
  const [testReport, setTestReport] = useState<ConnectionTestReport | null>(
    null,
  );
  const [testRequestId, setTestRequestId] = useState<string | null>(null);
  const [testBusy, setTestBusy] = useState(false);
  const [testError, setTestError] = useState<string | null>(null);
  const [deletePending, setDeletePending] = useState<ProviderConnection | null>(
    null,
  );
  const revision = useRef(0);
  const activeDiscovery = useRef<ConfigurationRequest | null>(null);
  const activeTest = useRef<ConfigurationRequest | null>(null);

  /** 请求草稿变更时取消在途网络操作并令旧响应失效。 */
  function invalidateRequests(clearDiscovered = false): void {
    revision.current += 1;
    for (const request of [activeDiscovery.current, activeTest.current]) {
      if (request)
        void agentApi.cancelConfiguration(request.id).catch(() => {});
    }
    activeDiscovery.current = null;
    activeTest.current = null;
    setDiscoveryRequestId(null);
    setDiscoveryCursor(null);
    setDiscoveryBusy(false);
    setDiscoveryError(null);
    setTestRequestId(null);
    setTestBusy(false);
    setTestReport(null);
    setTestError(null);
    setParameterErrors({});
    if (clearDiscovered) setDiscoveryModels([]);
  }

  /** 关闭 Hook 时尽力停止仍在运行的配置请求。 */
  useEffect(
    () => () => {
      for (const request of [activeDiscovery.current, activeTest.current]) {
        if (request)
          void agentApi.cancelConfiguration(request.id).catch(() => {});
      }
      activeDiscovery.current = null;
      activeTest.current = null;
    },
    [],
  );

  /** 返回某模型当前可用能力；未知配置只使用本地安全默认。 */
  function capabilitiesFor(model: ModelDraft): ModelConfig["capabilities"] {
    const saved = view?.settings.models.find(
      (item) =>
        item.id === model.id ||
        (item.providerId === provider.id && item.modelId === model.modelId),
    );
    if (
      saved &&
      capabilitiesMatchEndpoint(
        saved.capabilities,
        provider.baseUrl,
        model.protocol,
      )
    )
      return saved.capabilities;
    const discovered = discoveryModels.find(
      (item) => item.modelId === model.modelId,
    );
    if (
      discovered &&
      capabilitiesMatchEndpoint(
        discovered.capabilities,
        provider.baseUrl,
        model.protocol,
      )
    )
      return discovered.capabilities;
    const preset = view?.presets.find((item) => item.id === provider.presetId);
    const presetModel = preset?.models.find(
      (item) => item.modelId === model.modelId,
    );
    return presetModel &&
      capabilitiesMatchEndpoint(
        presetModel.capabilities,
        provider.baseUrl,
        model.protocol,
      )
      ? presetModel.capabilities
      : unknownModelCapabilities();
  }

  /** 根据正在编辑的连接及模型初始化表单，不读取已保存密钥。 */
  function openProviderEditor(connection: ProviderConnection | null): void {
    invalidateRequests(true);
    setLocalError(null);
    setNotice(null);
    setManualModelId("");
    setSearch("");
    if (!connection) {
      const initial = providerDraftForPreset(view?.presets[0] ?? null);
      setMode("provider");
      setProvider(initial.provider);
      setModels(initial.models);
      setSelectedModelId(initial.models[0]?.modelId ?? null);
      setModelDraft(null);
      setProviderRevision(null);
      setOriginalBaseUrl(null);
    } else {
      const savedModels =
        view?.settings.models.filter(
          (model) => model.providerId === connection.id,
        ) ?? [];
      const drafts = savedModels.map(modelDraftForEdit);
      const primary = drafts.find((model) => model.enabled) ?? drafts[0];
      setMode("provider");
      setProvider(
        providerDraftForEdit(
          connection,
          primary?.protocol ?? "chatCompletions",
        ),
      );
      setModels(drafts);
      setSelectedModelId(primary?.modelId ?? null);
      setModelDraft(null);
      setProviderRevision(connection.revision);
      setOriginalBaseUrl(connection.baseUrl);
    }
  }

  /** 打开新增供应商页，优先加载后端提供的首个核验预设。 */
  function add(): void {
    openProviderEditor(null);
  }

  /** 编辑已保存供应商及其当前模型集合。 */
  function editProvider(connection: ProviderConnection): () => void {
    return () => openProviderEditor(connection);
  }

  /** 打开已有连接下的模型编辑或手动添加页。 */
  function openModelEditor(
    connection: ProviderConnection,
    model: ModelConfig | null,
  ): void {
    const protocol = model?.protocol ?? "chatCompletions";
    invalidateRequests(false);
    setMode("model");
    setProvider(providerDraftForEdit(connection, protocol));
    setProviderRevision(connection.revision);
    setOriginalBaseUrl(connection.baseUrl);
    setModels([]);
    setSelectedModelId(null);
    setManualModelId("");
    setSearch("");
    setLocalError(null);
    setNotice(null);
    setModelDraft(
      model
        ? modelDraftForEdit(model)
        : {
            id: null,
            modelId: "",
            label: "",
            protocol,
            enabled: true,
            defaults: unknownModelParameters(),
            expectedRevision: null,
          },
    );
  }

  /** 编辑某个模型的独立参数，不回填系统密钥。 */
  function editModel(model: ModelConfig): () => void {
    return () => {
      const connection = view?.settings.providers.find(
        (item) => item.id === model.providerId,
      );
      if (connection) openModelEditor(connection, model);
    };
  }

  /** 为指定连接新增模型；未发现时仍允许手动填写稳定 modelId。 */
  function addModel(connection: ProviderConnection): () => void {
    return () => openModelEditor(connection, null);
  }

  /** 关闭配置页并清除临时 Key、草稿和在途测试。 */
  function close(): void {
    invalidateRequests(true);
    setMode(null);
    setProvider({
      id: null,
      presetId: null,
      name: "",
      baseUrl: "",
      protocol: "chatCompletions",
      apiKey: null,
      allowStoredKeyForChangedHost: false,
    });
    setModels([]);
    setModelDraft(null);
    setSelectedModelId(null);
    setProviderRevision(null);
    setOriginalBaseUrl(null);
    setLocalError(null);
    setManualModelId("");
    setSearch("");
  }

  /** 返回模型草稿所属预设，非预设连接不伪造能力。 */
  function selectedPreset(): ProviderPreset | null {
    return (
      view?.presets.find((preset) => preset.id === provider.presetId) ?? null
    );
  }

  /** 更新供应商字段；主机、协议或密钥变化会清空发现结果。 */
  function field(name: ProviderField) {
    return (event: ChangeEvent<HTMLInputElement>): void => {
      const value = event.target.value;
      const clearDiscovery = name !== "name";
      invalidateRequests(clearDiscovery);
      setProvider((current) => ({
        ...current,
        [name]: name === "apiKey" ? value || null : value,
        ...(name === "baseUrl"
          ? { apiKey: null, allowStoredKeyForChangedHost: false }
          : {}),
      }));
      setLocalError(null);
    };
  }

  /** 选择供应商预设并应用其协议、根地址和推荐模型。 */
  function presetChanged(event: ChangeEvent<HTMLSelectElement>): void {
    const preset =
      view?.presets.find((item) => item.id === event.target.value) ?? null;
    const next = providerDraftForPreset(preset);
    invalidateRequests(true);
    setProvider((current) => ({
      ...next.provider,
      id: current.id,
      apiKey: null,
      allowStoredKeyForChangedHost: false,
    }));
    if (provider.id === null) {
      setModels(next.models);
      setSelectedModelId(next.models[0]?.modelId ?? null);
    }
    setLocalError(null);
  }

  /** 更新模型目录搜索词，不改变保存草稿或进行网络请求。 */
  function searchChanged(event: ChangeEvent<HTMLInputElement>): void {
    setSearch(event.target.value);
  }

  /** 更新待添加的手动模型 ID，添加前不会发送或保存。 */
  function manualModelChanged(event: ChangeEvent<HTMLInputElement>): void {
    setManualModelId(event.target.value);
  }

  /** 协议只接受三个已定义值，并让用户重新确认模型选择。 */
  function protocolChanged(event: ChangeEvent<HTMLSelectElement>): void {
    const value = event.target.value;
    if (
      value !== "chatCompletions" &&
      value !== "responses" &&
      value !== "anthropicMessages"
    )
      return;
    invalidateRequests(true);
    setProvider((current) => ({ ...current, protocol: value }));
  }

  /** 明确允许已保存 Key 用于用户指定的新主机。 */
  function allowStoredKeyChanged(event: ChangeEvent<HTMLInputElement>): void {
    invalidateRequests(false);
    setProvider((current) => ({
      ...current,
      allowStoredKeyForChangedHost: event.target.checked,
    }));
  }

  /** 返回模型草稿列表中的模型并同步当前编辑焦点。 */
  function selectModel(model: ModelDraft): () => void {
    return () => {
      setSelectedModelId(model.modelId);
      setParameterErrors(modelParameterErrors(model, capabilitiesFor(model)));
    };
  }

  /** 返回当前表单中正在编辑的模型草稿。 */
  function selectedModel(): ModelDraft | null {
    if (mode === "model") return modelDraft;
    return models.find((model) => model.modelId === selectedModelId) ?? null;
  }

  /** 更新模型文本字段，同时使旧连接测试结果失效。 */
  function modelField(name: ModelField) {
    return (event: ChangeEvent<HTMLInputElement | HTMLSelectElement>): void => {
      const value = event.target.value;
      invalidateRequests(false);
      const update = (current: ModelDraft): ModelDraft => {
        if (name === "protocol") {
          if (
            value !== "chatCompletions" &&
            value !== "responses" &&
            value !== "anthropicMessages"
          )
            return current;
          return { ...current, protocol: value };
        }
        return name === "modelId"
          ? {
              ...current,
              modelId: value,
              label: current.label === current.modelId ? value : current.label,
            }
          : { ...current, label: value };
      };
      if (mode !== "model" && name === "modelId") setSelectedModelId(value);
      if (mode === "model")
        setModelDraft((current) => current && update(current));
      else
        setModels((current) =>
          current.map((item) =>
            item.modelId === selectedModelId ? update(item) : item,
          ),
        );
      setParameterErrors({});
    };
  }

  /** 更新一个模型参数，空输入表示恢复供应商默认。 */
  function parameterField(name: ModelParameterField) {
    return (event: ChangeEvent<HTMLInputElement | HTMLSelectElement>): void => {
      const input = event.target;
      const raw = input.value;
      let value: ModelParameters[ModelParameterField];
      if (name === "thinkingMode")
        value = raw === "enabled" || raw === "disabled" ? raw : null;
      else if (name === "reasoningEffort") value = raw || null;
      else if (input instanceof HTMLInputElement) value = numericValue(input);
      else return;
      invalidateRequests(false);
      const update = (current: ModelDraft): ModelDraft => ({
        ...current,
        defaults: { ...current.defaults, [name]: value },
      });
      if (mode === "model")
        setModelDraft((current) => current && update(current));
      else
        setModels((current) =>
          current.map((item) =>
            item.modelId === selectedModelId ? update(item) : item,
          ),
        );
      const current = selectedModel();
      if (current) {
        const next = update(current);
        setParameterErrors(modelParameterErrors(next, capabilitiesFor(next)));
      }
    };
  }

  /** 用户主动清除某项冲突参数，恢复为服务商默认值。 */
  function restoreParameter(name: ModelParameterField): () => void {
    return () => {
      const current = selectedModel();
      if (!current) return;
      const update = (model: ModelDraft): ModelDraft => ({
        ...model,
        defaults: { ...model.defaults, [name]: null },
      });
      invalidateRequests(false);
      if (mode === "model") setModelDraft(update(current));
      else
        setModels((items) =>
          items.map((item) =>
            item.modelId === selectedModelId ? update(item) : item,
          ),
        );
      setParameterErrors({});
    };
  }

  /** 开始使用手动填写的 modelId，拒绝空值和重复 ID。 */
  function addManualModel(): void {
    const modelId = manualModelId.trim();
    if (!modelId) {
      setLocalError("请填写服务商提供的模型 ID。");
      return;
    }
    if (models.some((model) => model.modelId === modelId)) {
      setLocalError("此模型 ID 已在当前连接中。");
      return;
    }
    const model = modelDraftForManual(
      modelId,
      provider.protocol,
      selectedPreset(),
    );
    invalidateRequests(false);
    setModels((current) => [...current, model]);
    setSelectedModelId(model.modelId);
    setManualModelId("");
    setLocalError(null);
  }

  /** 启用或停用发现模型，已有保存模型参数不会被发现结果覆盖。 */
  function discoveredModelChanged(model: DiscoveredModel) {
    return (event: ChangeEvent<HTMLInputElement>): void => {
      const existing = models.find((item) => item.modelId === model.modelId);
      if (existing) {
        invalidateRequests(false);
        setModels((current) =>
          current.map((item) =>
            item.modelId === model.modelId
              ? { ...item, enabled: event.target.checked }
              : item,
          ),
        );
        return;
      }
      if (!event.target.checked) return;
      const draft = modelDraftForDiscovered(
        model,
        provider.protocol,
        selectedPreset(),
      );
      invalidateRequests(false);
      setModels((current) => [...current, draft]);
      setSelectedModelId(draft.modelId);
    };
  }

  /** 停用已有模型不删除数据或聊天历史。 */
  function modelEnabledChanged(model: ModelDraft) {
    return (event: ChangeEvent<HTMLInputElement>): void => {
      invalidateRequests(false);
      setModels((current) =>
        current.map((item) =>
          item.modelId === model.modelId
            ? { ...item, enabled: event.target.checked }
            : item,
        ),
      );
    };
  }

  /** 根据当前连接、密钥和协议请求模型目录，旧页失败时保留现有数据。 */
  async function discoverModels(nextPage = false): Promise<void> {
    if (!enabled || busy || discoveryBusy) return;
    const keyError = providerKeyError(
      provider,
      originalBaseUrl,
      !!(provider.id && view?.providerHasApiKeys[provider.id]),
    );
    if (keyError) {
      setDiscoveryError(keyError);
      return;
    }
    const request = nextPage
      ? activeDiscovery.current
      : { id: newRequestId(), revision: revision.current };
    if (!request) return;
    if (!nextPage) {
      const test = activeTest.current;
      if (test) void agentApi.cancelConfiguration(test.id).catch(() => {});
      activeTest.current = null;
      setTestBusy(false);
      setTestRequestId(null);
      setTestReport(null);
      setTestError(null);
      activeDiscovery.current = request;
    }
    setDiscoveryBusy(true);
    setDiscoveryError(null);
    setDiscoveryRequestId(request.id);
    try {
      const page = await agentApi.listModels(
        { ...provider },
        request.id,
        nextPage ? discoveryCursor : null,
      );
      if (
        activeDiscovery.current?.id !== request.id ||
        revision.current !== request.revision ||
        !isCurrentConfigurationResponse(request.id, page.requestId)
      )
        return;
      setDiscoveryModels((current) => {
        const received = new Map(current.map((item) => [item.modelId, item]));
        for (const item of page.models) received.set(item.modelId, item);
        return [...received.values()];
      });
      setDiscoveryCursor(page.nextCursor);
      if (page.nextCursor === null) {
        activeDiscovery.current = null;
        setDiscoveryBusy(false);
        setDiscoveryRequestId(null);
      }
    } catch (cause) {
      if (
        activeDiscovery.current?.id === request.id &&
        revision.current === request.revision
      )
        setDiscoveryError(agentError(cause));
    } finally {
      if (activeDiscovery.current?.id === request.id) setDiscoveryBusy(false);
    }
  }

  /** 取消目录读取并丢弃之后到达的分页结果。 */
  function cancelDiscovery(): void {
    const request = activeDiscovery.current;
    if (!request) return;
    activeDiscovery.current = null;
    void agentApi.cancelConfiguration(request.id).catch(() => {});
    setDiscoveryBusy(false);
    setDiscoveryRequestId(null);
    setDiscoveryCursor(null);
  }

  /** 对当前未保存连接和模型做分阶段测试，不先写入任何配置。 */
  async function testCurrentModel(): Promise<void> {
    const model = selectedModel();
    if (!enabled || busy || testBusy || !model) return;
    const keyError = providerKeyError(
      provider,
      originalBaseUrl,
      !!(provider.id && view?.providerHasApiKeys[provider.id]),
    );
    if (keyError) {
      setTestError(keyError);
      return;
    }
    const invalid = modelParameterErrors(model, capabilitiesFor(model));
    setParameterErrors(invalid);
    if (Object.keys(invalid).length > 0) {
      setTestError("请先修正模型参数冲突，再测试连接。");
      return;
    }
    const request = { id: newRequestId(), revision: revision.current };
    const discovery = activeDiscovery.current;
    if (discovery) {
      void agentApi.cancelConfiguration(discovery.id).catch(() => {});
      activeDiscovery.current = null;
      setDiscoveryBusy(false);
      setDiscoveryRequestId(null);
    }
    activeTest.current = request;
    setTestReport(null);
    setTestError(null);
    setTestBusy(true);
    setTestRequestId(request.id);
    try {
      const report = await agentApi.testModel(
        { ...provider },
        { ...model, defaults: { ...model.defaults } },
        request.id,
      );
      if (
        activeTest.current?.id === request.id &&
        revision.current === request.revision &&
        isCurrentConfigurationResponse(request.id, report.requestId)
      )
        setTestReport(report);
    } catch (cause) {
      if (
        activeTest.current?.id === request.id &&
        revision.current === request.revision
      )
        setTestError(agentError(cause));
    } finally {
      if (activeTest.current?.id === request.id) {
        activeTest.current = null;
        setTestBusy(false);
        setTestRequestId(null);
      }
    }
  }

  /** 取消当前连接测试；迟到响应不会重新显示旧报告。 */
  function cancelTest(): void {
    const request = activeTest.current;
    if (!request) return;
    activeTest.current = null;
    void agentApi.cancelConfiguration(request.id).catch(() => {});
    setTestBusy(false);
    setTestRequestId(null);
    setTestReport(null);
  }

  /** 验证供应商密钥和已启用模型默认值后原子保存连接配置。 */
  async function saveProvider(): Promise<boolean> {
    if (!enabled || busy || mode !== "provider") return false;
    const keyError = providerKeyError(
      provider,
      originalBaseUrl,
      !!(provider.id && view?.providerHasApiKeys[provider.id]),
    );
    if (keyError) {
      setLocalError(keyError);
      return false;
    }
    if (
      models
        .filter((model) => model.enabled)
        .some((model) => !model.modelId.trim() || !model.label.trim())
    ) {
      setLocalError("每个启用的模型都需要有效的名称和模型 ID。");
      return false;
    }
    const errors = models.reduce<ModelParameterErrors>((all, model) => {
      if (!model.enabled) return all;
      const current = modelParameterErrors(model, capabilitiesFor(model));
      return Object.keys(current).length === 0 ? all : { ...all, ...current };
    }, {});
    if (Object.keys(errors).length > 0) {
      setParameterErrors(errors);
      setLocalError("启用的模型存在参数冲突，请修复或恢复为供应商默认。");
      return false;
    }
    invalidateRequests(false);
    setBusy(true);
    setLocalError(null);
    setError(null);
    setNotice(null);
    try {
      const result = await agentApi.saveProvider(
        saveProviderRequest(provider, providerRevision, models),
      );
      setView(result);
      setProvider((current) => ({ ...current, apiKey: null }));
      setNotice("供应商连接和模型配置已保存。");
      close();
      return true;
    } catch (cause) {
      setError(agentError(cause));
      return false;
    } finally {
      setBusy(false);
    }
  }

  /** 保存或编辑单个模型，不写入临时 Key 或其他模型草稿。 */
  async function saveModel(): Promise<boolean> {
    if (!enabled || busy || mode !== "model" || !modelDraft || !provider.id)
      return false;
    const keyError = providerKeyError(
      provider,
      originalBaseUrl,
      !!view?.providerHasApiKeys[provider.id],
    );
    if (keyError) {
      setLocalError(keyError);
      return false;
    }
    if (!modelDraft.modelId.trim() || !modelDraft.label.trim()) {
      setLocalError("请填写有效的模型名称和模型 ID。");
      return false;
    }
    const errors = modelParameterErrors(
      modelDraft,
      capabilitiesFor(modelDraft),
    );
    setParameterErrors(errors);
    if (Object.keys(errors).length > 0) {
      setLocalError("请修正模型参数冲突，或明确恢复为供应商默认。");
      return false;
    }
    invalidateRequests(false);
    setBusy(true);
    setLocalError(null);
    setError(null);
    setNotice(null);
    const request: SaveModel = { ...modelDraft, providerId: provider.id };
    try {
      setView(await agentApi.saveModel(request));
      setNotice("模型配置已保存。");
      close();
      return true;
    } catch (cause) {
      setError(agentError(cause));
      return false;
    } finally {
      setBusy(false);
    }
  }

  /** 根据当前配置模式提交供应商原子事务或单模型更新。 */
  async function save(): Promise<boolean> {
    return mode === "provider" ? saveProvider() : saveModel();
  }

  /** 表单提交入口阻止浏览器导航并运行当前模式的保存操作。 */
  function submit(event: FormEvent<HTMLFormElement>): void {
    event.preventDefault();
    void save();
  }

  /** 将预设模型加入供应商草稿，不覆盖同 ID 模型的已编辑参数。 */
  function presetModelChanged(model: PresetModel) {
    return (event: ChangeEvent<HTMLInputElement>): void => {
      const existing = models.find((item) => item.modelId === model.modelId);
      if (existing) {
        if (existing.id === null && !event.target.checked) {
          invalidateRequests(false);
          setModels((current) =>
            current.filter((item) => item.modelId !== model.modelId),
          );
          if (selectedModelId === model.modelId) setSelectedModelId(null);
        } else {
          invalidateRequests(false);
          setModels((current) =>
            current.map((item) =>
              item.modelId === model.modelId
                ? { ...item, enabled: event.target.checked }
                : item,
            ),
          );
        }
        return;
      }
      if (!event.target.checked) return;
      const draft = modelDraftForPreset(model);
      invalidateRequests(false);
      setModels((current) => [...current, draft]);
      setSelectedModelId(draft.modelId);
    };
  }

  /** 模型页需要更换密钥时转到连接编辑，不把 Key 当作模型字段保存。 */
  function editCurrentProvider(): void {
    if (!provider.id) return;
    const connection = view?.settings.providers.find(
      (item) => item.id === provider.id,
    );
    if (connection) openProviderEditor(connection);
  }

  /** 切换模型启用状态并通过模型接口保存。 */
  function toggleSavedModel(model: ModelConfig) {
    return (event: ChangeEvent<HTMLInputElement>): void => {
      const connection = view?.settings.providers.find(
        (item) => item.id === model.providerId,
      );
      if (!connection) return;
      const request: SaveModel = {
        ...modelDraftForEdit(model),
        enabled: event.target.checked,
        providerId: connection.id,
      };
      void runSaveModel(request);
    };
  }

  /** 保存外部模型状态切换并保留其他配置快照。 */
  async function runSaveModel(request: SaveModel): Promise<void> {
    if (!enabled || busy) return;
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      setView(await agentApi.saveModel(request));
      setNotice(
        request.enabled ? "模型已启用。" : "模型已停用；聊天记录仍保留。",
      );
    } catch (cause) {
      setError(agentError(cause));
    } finally {
      setBusy(false);
    }
  }

  /** 打开供应商删除确认并保留其模型数供 UI 显示。 */
  function requestDeleteProvider(connection: ProviderConnection) {
    return () => setDeletePending(connection);
  }

  /** 取消删除供应商，不修改配置或凭据。 */
  function keepProvider(): void {
    setDeletePending(null);
  }

  /** 删除连接时显式传入 revision，历史聊天仍由后端保留。 */
  async function confirmDeleteProvider(): Promise<void> {
    if (!enabled || busy || !deletePending) return;
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      setView(
        await agentApi.deleteProvider(deletePending.id, deletePending.revision),
      );
      setNotice("供应商连接已删除，相关聊天记录仍保留。");
      setDeletePending(null);
    } catch (cause) {
      setError(agentError(cause));
    } finally {
      setBusy(false);
    }
  }

  /** 当前草稿关联的发现结果可按名称或稳定 ID过滤。 */
  function visibleDiscoveredModels(): DiscoveredModel[] {
    const query = search.trim().toLocaleLowerCase();
    if (!query) return discoveryModels;
    return discoveryModels.filter(
      (item) =>
        item.modelId.toLocaleLowerCase().includes(query) ||
        item.label.toLocaleLowerCase().includes(query),
    );
  }

  const currentModel = selectedModel();
  const currentCapabilities = currentModel
    ? capabilitiesFor(currentModel)
    : unknownModelCapabilities();
  const currentAvailability = currentModel
    ? modelParameterAvailability(currentModel, currentCapabilities)
    : null;
  const currentErrors = currentModel
    ? modelParameterErrors(currentModel, currentCapabilities)
    : {};
  const keyError = providerKeyError(
    provider,
    originalBaseUrl,
    !!(provider.id && view?.providerHasApiKeys[provider.id]),
  );
  return {
    mode,
    setupOpen: mode !== null,
    provider,
    providerRevision,
    originalBaseUrl,
    hasStoredKey: !!(provider.id && view?.providerHasApiKeys[provider.id]),
    keyError,
    models,
    model: currentModel,
    modelCapabilities: currentCapabilities,
    modelAvailability: currentAvailability,
    modelErrors: { ...currentErrors, ...parameterErrors },
    localError,
    manualModelId,
    search,
    discoveryModels: visibleDiscoveredModels(),
    discoveryRequestId,
    discoveryCursor,
    discoveryBusy,
    discoveryError,
    testReport,
    testRequestId,
    testBusy,
    testError,
    busy,
    deletePending,
    field,
    presetChanged,
    searchChanged,
    manualModelChanged,
    protocolChanged,
    allowStoredKeyChanged,
    editProvider,
    addModel,
    editModel,
    add,
    close,
    save,
    submit,
    modelField,
    parameterField,
    restoreParameter,
    selectModel,
    setManualModelId,
    setSearch,
    addManualModel,
    presetModelChanged,
    discoveredModelChanged,
    modelEnabledChanged,
    discoverModels,
    cancelDiscovery,
    testCurrentModel,
    cancelTest,
    requestDeleteProvider,
    keepProvider,
    confirmDeleteProvider,
    toggleSavedModel,
    editCurrentProvider,
    storedKeyConsentRequired:
      provider.id !== null &&
      !!view?.providerHasApiKeys[provider.id] &&
      !provider.apiKey?.trim() &&
      providerHostChanged(originalBaseUrl, provider.baseUrl),
    selectedPreset: selectedPreset(),
  };
}
