import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type ChangeEvent,
  type FormEvent,
  type KeyboardEvent,
  type UIEvent,
} from "react";
import {
  agentApi,
  agentError,
  listenAgent,
  listenAgentWrites,
  listenAgentWorkspacesChanged,
} from "../services/agent";
import type {
  AgentWorkspace,
  ChatRecord,
  ChatSession,
  ChatView,
  ModelConfig,
  RunNotice,
  RunSnapshot,
} from "../types/agent";
import type { ModelParameters } from "../types/agentModels";
import type { ChatSearchResult } from "../types/agentMemory";
import { useAgentSettings } from "./useAgentSettings";
import { useAgentSearch } from "./useAgentSearch";
import { useAgentContext } from "./useAgentContext";
import { AgentChatMemory, titleValidation } from "../ui/agentChatMemory";
import { draftAfterSend, handleComposerKeyDown } from "../ui/agentComposer";
import {
  chatConfigurationForModel,
  chatConfigurationMatchesSession,
  chatConfigurationForSession,
  currentWorkspaces,
  effectiveChatParameters,
  newChatConfiguration,
  restoreChatParameter,
  sessionWorkspace,
  updateChatParameter,
  type ChatConfigurationDraft,
} from "../ui/agentChatConfiguration";
import {
  modelParameterAvailability,
  modelParameterErrors,
  type ModelParameterEditorController,
} from "../ui/agentModelConfiguration";
/** 管理本地聊天与运行事件；不持久化前端副本或密钥。 */
export function useAgent(
  enabled: boolean,
  repositoryPath: string | null,
  refreshRepository: () => Promise<void>,
) {
  const settings = useAgentSettings(enabled);
  const [chats, setChats] = useState<ChatSession[]>([]);
  const [workspaces, setWorkspaces] = useState<AgentWorkspace[]>([]);
  const workspacesRef = useRef<AgentWorkspace[]>([]);
  workspacesRef.current = workspaces;
  const [workspaceId, setWorkspaceId] = useState("default");
  const [workspaceBusy, setWorkspaceBusy] = useState(false);
  const [configurationBusy, setConfigurationBusy] = useState(false);
  const [chatConfiguration, setChatConfiguration] =
    useState<ChatConfigurationDraft | null>(null);
  const [searchTargetSequence, setSearchTargetSequence] = useState<
    number | null
  >(null);
  const [, setNewConfigurationRevision] = useState(0);
  const [unavailable, setUnavailable] = useState<string[]>([]);
  const [record, setRecord] = useState<ChatRecord | null>(null);
  const [run, setRun] = useState<RunSnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [draft, setDraft] = useState("");
  const [title, setTitle] = useState("");
  const [renameTarget, setRenameTarget] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [archived, setArchived] = useState(false);
  const [deletePending, setDeletePending] = useState<string | null>(null);
  const selected = useRef<string | null>(null);
  const newConfigurations = useRef(new Map<string, ChatConfigurationDraft>());
  const lastModels = useRef(new Map<string, string>());
  const workspaceGeneration = useRef(0);
  const configurationGeneration = useRef(0);
  const enterHeld = useRef(false);
  const memory = useRef(new AgentChatMemory());
  const generation = useRef(0);
  const progressVersion = useRef(0);
  const finished = useRef(new Set<string>());
  const end = useRef<HTMLDivElement | null>(null);
  const messagesRoot = useRef<HTMLDivElement | null>(null);
  const follow = useRef(true);
  const refreshRef = useRef(refreshRepository);
  refreshRef.current = refreshRepository;
  /** 列表独立刷新，不覆盖用户正在阅读的聊天。 */
  const refreshList = useCallback(async () => {
    const result = await agentApi.chats();
    setChats(result.chats.sort((a, b) => b.updatedAt - a.updatedAt));
    setUnavailable(result.unavailable);
  }, []);
  /** 只展示后端签发的默认工作区与唯一当前文件夹。 */
  const refreshWorkspaces = useCallback(async () => {
    const result = currentWorkspaces(await agentApi.listWorkspaces());
    const previous = workspacesRef.current;
    if (
      previous.length !== result.length ||
      result.some(
        (item, index) =>
          item.id !== previous[index]?.id ||
          item.rootPath !== previous[index]?.rootPath ||
          item.generation !== previous[index]?.generation,
      )
    )
      workspaceGeneration.current += 1;
    workspacesRef.current = result;
    setWorkspaces(result);
    setWorkspaceId((current) =>
      result.some((workspace) => workspace.id === current)
        ? current
        : (result[0]?.id ?? "default"),
    );
    return result;
  }, []);
  /** 返回指定工作区的新聊天草稿配置，优先最近选择再回退全局默认。 */
  function newConfigurationFor(
    workspace: AgentWorkspace,
  ): ChatConfigurationDraft {
    const saved = newConfigurations.current.get(workspace.id);
    const models = settings.view?.settings.models ?? [];
    const lastModel = models.find(
      (model) =>
        model.enabled && model.id === lastModels.current.get(workspace.id),
    );
    if (saved)
      return lastModel && saved.modelConfigId !== lastModel.id
        ? chatConfigurationForModel(saved, lastModel.id)
        : saved;
    const modelConfigId =
      lastModel?.id ??
      models.find(
        (model) =>
          model.enabled &&
          model.id === settings.view?.settings.activeModelConfigId,
      )?.id ??
      models.find((model) => model.enabled)?.id ??
      null;
    return newChatConfiguration(
      modelConfigId,
      settings.view?.settings.defaultPermissionMode ?? "requestApproval",
    );
  }
  /** 新聊天草稿使用工作区 ID 隔离，路径切换不会覆盖别处输入。 */
  function workspaceDraftScope(id: string): string {
    return `workspace:${id}`;
  }
  /** 读取指定聊天的真实根路径供聊天草稿键使用。 */
  function chatRootPath(id: string): string | null {
    if (record?.session.id === id) return record.session.repositoryPath;
    return chats.find((chat) => chat.id === id)?.repositoryPath ?? null;
  }
  /** 返回当前聊天或工作区草稿的独立内存作用域。 */
  function activeDraftScope(): string | null {
    return selected.current
      ? chatRootPath(selected.current)
      : workspaceDraftScope(workspaceId);
  }
  /** 用请求代次隔离快速切换；活动通知比读取开始后的旧快照更新。 */
  const load = useCallback(
    async (id: string, switching = false, rootPath: string | null = null) => {
      if (switching) {
        selected.current = id;
        setDraft(memory.current.draft(id, rootPath));
        setChatConfiguration(null);
        setRecord(null);
        setRun(null);
        setTitle("");
        setRenameTarget(null);
        setDeletePending(null);
        follow.current = true;
      }
      const version = ++generation.current;
      const progress = progressVersion.current;
      memory.current.beginLoad(id);
      let view: ChatView;
      try {
        view = await agentApi.read(id);
      } catch (cause) {
        if (version === generation.current) memory.current.endLoad(id);
        throw cause;
      }
      if (selected.current !== id || version !== generation.current) return;
      const loaded = memory.current.merge(view.record, null);
      memory.current.endLoad(id);
      setRecord((current) => memory.current.merge(loaded, current));
      if (selected.current === id)
        setChatConfiguration(chatConfigurationForSession(view.record.session));
      if (switching) setTitle(view.record.session.title);
      if (progress === progressVersion.current)
        setRun(
          view.run && !finished.current.has(view.run.runId) ? view.run : null,
        );
    },
    [],
  );
  /** 接收安全持久事件，忽略其他聊天和已结束运行的迟到进度。 */
  const receive = useCallback(
    (notice: RunNotice) => {
      if (notice.kind === "record") {
        const event = notice.event;
        if (event.kind === "runEnded") {
          finished.current.add(event.runId);
          void refreshList().catch((cause: unknown) =>
            setError(agentError(cause)),
          );
        }
        if (selected.current !== event.sessionId) return;
        memory.current.receive(event);
        setRecord((current) =>
          current?.session.id === event.sessionId
            ? {
                ...current,
                events: [
                  ...current.events.filter(
                    (value) => value.sequence !== event.sequence,
                  ),
                  event,
                ].sort((a, b) => a.sequence - b.sequence),
              }
            : current,
        );
        if (event.kind === "runEnded") {
          progressVersion.current++;
          setRun(null);
        }
      } else if (notice.kind === "progress") {
        if (
          selected.current !== notice.sessionId ||
          finished.current.has(notice.state.runId)
        )
          return;
        progressVersion.current++;
        setRun(notice.state);
      } else {
        finished.current.add(notice.runId);
        void settings.refreshSettings();
        if (selected.current !== notice.sessionId) return;
        progressVersion.current++;
        setRun(null);
        if (notice.error) setError(notice.error.message);
        void load(notice.sessionId).catch((cause: unknown) =>
          setError(agentError(cause)),
        );
        void refreshList().catch((cause: unknown) =>
          setError(agentError(cause)),
        );
      }
    },
    [load, refreshList, settings.refreshSettings],
  );
  /** 在首个请求前订阅事件；卸载时移除所有监听。 */
  useEffect(() => {
    if (!enabled) return;
    let disposed = false;
    const cleanup: (() => void)[] = [];
    void Promise.allSettled([
      listenAgent(receive),
      listenAgentWrites(() => {
        void refreshRef.current();
      }),
      listenAgentWorkspacesChanged(() => {
        void refreshWorkspaces()
          .then(() => refreshList())
          .catch((cause: unknown) => setError(agentError(cause)));
      }),
    ])
      .then((results) => {
        for (const result of results) {
          if (result.status === "fulfilled") {
            if (disposed) result.value();
            else cleanup.push(result.value);
          } else if (!disposed) setError(agentError(result.reason));
        }
      })
      .catch((cause: unknown) => setError(agentError(cause)));
    void refreshList().catch((cause: unknown) => setError(agentError(cause)));
    void refreshWorkspaces().catch((cause: unknown) =>
      setError(agentError(cause)),
    );
    return () => {
      disposed = true;
      cleanup.forEach((stop) => stop());
    };
  }, [enabled, receive, refreshList, refreshWorkspaces]);
  /** 没有聊天时按项目恢复草稿，切换项目不能带走未发送内容。 */
  useEffect(() => {
    if (selected.current === null)
      setDraft(memory.current.draft(null, workspaceDraftScope(workspaceId)));
  }, [workspaceId, repositoryPath]);
  /** 阅读历史时不强行滚动，只有接近底部时跟随新消息。 */
  useEffect(() => {
    if (follow.current) end.current?.scrollIntoView({ block: "end" });
  }, [record?.events.length, run?.text, run?.phase]);
  /** 搜索定位命中消息后居中显示，避免自动滚动覆盖目标。 */
  useEffect(() => {
    if (searchTargetSequence === null) return;
    const target = messagesRoot.current?.querySelector<HTMLElement>(
      `[data-agent-sequence="${searchTargetSequence}"]`,
    );
    if (target) {
      let parent = target.parentElement;
      while (parent) {
        if (parent instanceof HTMLDetailsElement) parent.open = true;
        parent = parent.parentElement;
      }
      target.scrollIntoView({ block: "center" });
      setSearchTargetSequence(null);
    }
  }, [searchTargetSequence, record?.session.id, record?.events.length]);
  /** 串行处理前端动作，保留输入与错误，不吞掉后端失败。 */
  async function action(task: () => Promise<void>) {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      await task();
    } catch (cause) {
      setError(agentError(cause));
    } finally {
      setBusy(false);
    }
  }
  /** 选择聊天只加载本地记录，不自动恢复或重放运行。 */
  function select(chat: ChatSession) {
    return () => {
      void action(() => load(chat.id, true, chat.repositoryPath));
    };
  }
  /** 从搜索结果打开聊天并定位到命中的消息序号。 */
  function selectSearchResult(result: ChatSearchResult): void {
    const chat = chats.find((item) => item.id === result.chatId);
    if (!chat) return;
    follow.current = false;
    void action(async () => {
      await load(chat.id, true, chat.repositoryPath);
      setSearchTargetSequence(result.sequence);
    });
  }
  /** 返回当前工作区已启用模型，供聊天级选择使用。 */
  function enabledModels(): ModelConfig[] {
    return (
      settings.view?.settings.models.filter((model) => model.enabled) ?? []
    );
  }
  /** 新建聊天使用该工作区最近选择，否则使用全局默认模型。 */
  function selectedNewChatConfiguration(): ChatConfigurationDraft {
    const workspace = workspaces.find((item) => item.id === workspaceId);
    if (workspace) return newConfigurationFor(workspace);
    return newChatConfiguration(
      settings.view?.settings.models.find(
        (model) =>
          model.enabled &&
          model.id === settings.view?.settings.activeModelConfigId,
      )?.id ??
        settings.view?.settings.models.find((model) => model.enabled)?.id ??
        null,
      settings.view?.settings.defaultPermissionMode ?? "requestApproval",
    );
  }
  /** 保存新聊天配置草稿并刷新视图，不调用全局 profile 设置。 */
  function setConfigurationDraft(next: ChatConfigurationDraft): void {
    if (record) setChatConfiguration(next);
    else {
      newConfigurations.current.set(workspaceId, next);
      setNewConfigurationRevision((value) => value + 1);
    }
  }
  /** 校验模型仍可用，删除或停用后保留草稿并提示重新选择。 */
  function validateSelectedModel(
    configuration: ChatConfigurationDraft,
  ): ModelConfig {
    const model = enabledModels().find(
      (item) => item.id === configuration.modelConfigId,
    );
    if (model) return model;
    throw new Error("所选模型已不可用，请重新选择后再继续。");
  }
  /** 校验新聊天参数并生成一次原子创建会话的配置字段。 */
  function newChatRequest(configuration: ChatConfigurationDraft): {
    modelConfigId: string | null;
    permissionMode: ChatConfigurationDraft["permissionMode"];
    parameters: ModelParameters | null;
  } {
    if (configuration.modelConfigId === null) {
      return {
        modelConfigId: null,
        permissionMode: configuration.permissionMode,
        parameters: null,
      };
    }
    const model = validateSelectedModel(configuration);
    const parameters = effectiveChatParameters(configuration, model);
    const modelDraft = {
      id: model.id,
      modelId: model.modelId,
      label: model.label,
      protocol: model.protocol,
      enabled: model.enabled,
      defaults: parameters,
      expectedRevision: model.revision,
    };
    if (
      Object.keys(modelParameterErrors(modelDraft, model.capabilities)).length
    )
      throw new Error("聊天参数与模型能力冲突，请修正或恢复模型默认值。");
    return {
      modelConfigId: configuration.modelConfigId,
      permissionMode: configuration.permissionMode,
      parameters: configuration.parameters,
    };
  }
  /** 为新聊天的记忆设置准备会话，不发送消息或清除工作区草稿。 */
  async function ensureChatForMemory(): Promise<ChatSession> {
    if (record) return record.session;
    if (busy) throw new Error("聊天正在切换，请稍后再试。");
    const workspace = workspaces.find((item) => item.id === workspaceId);
    if (!workspace) throw new Error("工作区列表暂不可用，请刷新后重试。");
    const configuration = newConfigurationFor(workspace);
    const selectionGeneration = workspaceGeneration.current;
    setBusy(true);
    setError(null);
    try {
      const request = newChatRequest(configuration);
      const created = await agentApi.createChat(
        workspace.id,
        request.modelConfigId,
        request.permissionMode,
        request.parameters,
      );
      rememberCreatedChat(created);
      await refreshList().catch((cause: unknown) =>
        setError(agentError(cause)),
      );
      if (selectionGeneration !== workspaceGeneration.current)
        throw new Error("创建期间工作区已切换，请重新打开记忆设置。");
      await load(created.id, true, created.repositoryPath);
      return created;
    } catch (cause) {
      setError(agentError(cause));
      throw cause;
    } finally {
      setBusy(false);
    }
  }
  /** 把刚创建的会话放入列表，读取失败后用户仍可选中并重试。 */
  function rememberCreatedChat(session: ChatSession): void {
    setChats((current) => [
      session,
      ...current.filter((chat) => chat.id !== session.id),
    ]);
  }
  /** 将记忆设置变更返回的会话写入当前聊天快照。 */
  function updateChatSession(session: ChatSession): void {
    setChats((current) =>
      current.map((chat) => (chat.id === session.id ? session : chat)),
    );
    setRecord((current) =>
      current?.session.id === session.id ? { ...current, session } : current,
    );
  }
  /** 新建聊天绑定所选工作区及模型；失败时保留该工作区草稿。 */
  function create() {
    if (configurationBusy) return;
    if (enabledModels().length === 0) {
      settings.add();
      return;
    }
    const workspace = workspaces.find((item) => item.id === workspaceId);
    if (!workspace) {
      setError("工作区列表暂不可用，请刷新后重试。");
      return;
    }
    const configuration = newConfigurationFor(workspace);
    try {
      newChatRequest(configuration);
    } catch (cause) {
      setError(agentError(cause));
      return;
    }
    const selectionGeneration = workspaceGeneration.current;
    void action(async () => {
      const request = newChatRequest(configuration);
      const created = await agentApi.createChat(
        workspace.id,
        request.modelConfigId,
        request.permissionMode,
        request.parameters,
      );
      rememberCreatedChat(created);
      await refreshList().catch((cause: unknown) =>
        setError(agentError(cause)),
      );
      if (selectionGeneration !== workspaceGeneration.current) return;
      await load(created.id, true, created.repositoryPath);
    });
  }
  /** 返回当前聊天或当前工作区的模型与权限草稿。 */
  function activeConfigurationDraft(): ChatConfigurationDraft {
    if (record)
      return chatConfiguration ?? chatConfigurationForSession(record.session);
    return selectedNewChatConfiguration();
  }
  /** 选择模型只更新当前聊天或工作区新聊天草稿，不修改全局默认。 */
  function changeModelConfig(event: ChangeEvent<HTMLSelectElement>): void {
    if (run) return;
    const id = event.target.value || null;
    if (id && !enabledModels().some((model) => model.id === id)) return;
    const next = chatConfigurationForModel(activeConfigurationDraft(), id);
    const targetWorkspace = record?.session.workspaceId ?? workspaceId;
    if (id) lastModels.current.set(targetWorkspace, id);
    setConfigurationDraft(next);
  }
  /** 修改当前聊天或新聊天的权限草稿，运行期间不允许变更。 */
  function changePermissionMode(event: ChangeEvent<HTMLSelectElement>): void {
    if (run) return;
    const value = event.target.value;
    if (
      value !== "requestApproval" &&
      value !== "autoApprove" &&
      value !== "fullAccess"
    )
      return;
    setConfigurationDraft({
      ...activeConfigurationDraft(),
      permissionMode: value,
    });
  }
  /** 返回本聊天/工作区新聊天的参数草稿与核验能力。 */
  function chatParameterEditor(): ModelParameterEditorController {
    const configuration = activeConfigurationDraft();
    const model = settings.view?.settings.models.find(
      (item) => item.id === configuration.modelConfigId,
    );
    if (!model)
      return {
        model: null,
        modelCapabilities: {
          contextWindow: null,
          maxOutputTokens: null,
          supportedEfforts: [],
          thinkingModes: [],
          thinkingBudget: false,
          toolSupport: null,
          samplingSupport: null,
          source: "unknown",
          verifiedAt: null,
          endpoint: null,
          parameterStyle: "generic",
        },
        modelAvailability: null,
        modelErrors: {},
        modelField: () => () => {},
        parameterField: () => () => {},
        restoreParameter: () => () => {},
      };
    const defaults = effectiveChatParameters(configuration, model);
    const modelDraft = {
      id: model.id,
      modelId: model.modelId,
      label: model.label,
      protocol: model.protocol,
      enabled: model.enabled,
      defaults,
      expectedRevision: model.revision,
    };
    return {
      model: modelDraft,
      modelCapabilities: model.capabilities,
      modelAvailability: modelParameterAvailability(
        modelDraft,
        model.capabilities,
      ),
      modelErrors: modelParameterErrors(modelDraft, model.capabilities),
      modelField: () => () => {},
      parameterField: (field) => (event) => {
        if (run) return;
        const raw = event.target.value;
        let value: ModelParameters[typeof field];
        if (field === "thinkingMode")
          value = (
            raw === "enabled" || raw === "disabled" ? raw : null
          ) as ModelParameters[typeof field];
        else if (field === "reasoningEffort")
          value = (raw || null) as ModelParameters[typeof field];
        else if (event.target instanceof HTMLInputElement) {
          value = (
            raw.trim() === "" || !Number.isFinite(event.target.valueAsNumber)
              ? null
              : event.target.valueAsNumber
          ) as ModelParameters[typeof field];
        } else return;
        setConfigurationDraft(
          updateChatParameter(configuration, model, field, value),
        );
      },
      restoreParameter: (field) => () => {
        if (!run)
          setConfigurationDraft(
            restoreChatParameter(configuration, model, field),
          );
      },
    };
  }
  /** 保存当前聊天完整参数快照；失败时保留尚未提交的草稿。 */
  async function saveChatConfiguration(): Promise<boolean> {
    if (!record || run || configurationBusy) return false;
    const configuration = activeConfigurationDraft();
    let model: ModelConfig;
    try {
      model = validateSelectedModel(configuration);
    } catch (cause) {
      setError(agentError(cause));
      return false;
    }
    const modelDraft = {
      id: model.id,
      modelId: model.modelId,
      label: model.label,
      protocol: model.protocol,
      enabled: model.enabled,
      defaults: effectiveChatParameters(configuration, model),
      expectedRevision: model.revision,
    };
    if (
      Object.keys(modelParameterErrors(modelDraft, model.capabilities)).length
    ) {
      setError("当前聊天参数与模型能力冲突，请修正或恢复模型默认值。");
      return false;
    }
    if (chatConfigurationMatchesSession(configuration, record.session))
      return true;
    const id = record.session.id;
    const requestGeneration = ++configurationGeneration.current;
    setConfigurationBusy(true);
    setError(null);
    try {
      const session = await agentApi.configureChat({
        id,
        modelConfigId: configuration.modelConfigId,
        permissionMode: configuration.permissionMode,
        parameters: configuration.parameters,
      });
      if (
        selected.current === id &&
        requestGeneration === configurationGeneration.current
      ) {
        setRecord((current) =>
          current?.session.id === id ? { ...current, session } : current,
        );
        setChats((current) =>
          current.map((chat) => (chat.id === id ? session : chat)),
        );
        setChatConfiguration(chatConfigurationForSession(session));
      }
      return true;
    } catch (cause) {
      setError(agentError(cause));
      return false;
    } finally {
      if (requestGeneration === configurationGeneration.current)
        setConfigurationBusy(false);
    }
  }
  /** 切换新聊天工作区并恢复其独立草稿，不改变当前聊天或活动运行。 */
  function selectWorkspace(event: ChangeEvent<HTMLSelectElement>): void {
    const next = workspaces.find((item) => item.id === event.target.value);
    if (!next) return;
    workspaceGeneration.current += 1;
    setWorkspaceId(next.id);
    if (!selected.current)
      setDraft(memory.current.draft(null, workspaceDraftScope(next.id)));
  }
  /** 通过桌面原生选择器替换唯一当前文件夹工作区。 */
  async function chooseFolder(): Promise<void> {
    setWorkspaceBusy(true);
    setError(null);
    try {
      const next = currentWorkspaces(await agentApi.chooseFolder());
      workspacesRef.current = next;
      setWorkspaces(next);
      workspaceGeneration.current += 1;
      const folder = next.find((item) => item.kind === "folder");
      const selectedWorkspace =
        folder ?? next.find((item) => item.id === "default");
      if (selectedWorkspace) {
        setWorkspaceId(selectedWorkspace.id);
        if (!selected.current)
          setDraft(
            memory.current.draft(
              null,
              workspaceDraftScope(selectedWorkspace.id),
            ),
          );
      }
      await refreshList();
    } catch (cause) {
      setError(agentError(cause));
    } finally {
      setWorkspaceBusy(false);
    }
  }
  /** 发送时才创建缺失聊天，输入在成功接受后清空。 */
  async function send(event?: FormEvent) {
    event?.preventDefault();
    if (!draft.trim() || run || busy || configurationBusy) return;
    if (enabledModels().length === 0) {
      if (record) setError("当前聊天模型已不可用，请先配置并重新选择模型。");
      else settings.add();
      return;
    }
    const configuration = activeConfigurationDraft();
    try {
      validateSelectedModel(configuration);
    } catch (cause) {
      setError(agentError(cause));
      return;
    }
    await action(async () => {
      const references = context.references;
      if (!context.canSendReferences)
        throw new Error("引用尚未完成校验，请先等待校验结果后重试。");
      let id = selected.current;
      const sourceChatId = id;
      const sourceScope = activeDraftScope();
      const text = draft;
      let session = record?.session ?? null;
      if (!id) {
        const workspace = workspaces.find((item) => item.id === workspaceId);
        if (!workspace) throw new Error("工作区已失效，请重新选择后重试。");
        const selectionGeneration = workspaceGeneration.current;
        context.preserveReferencesForChatCreate();
        try {
          const request = newChatRequest(configuration);
          const created = await agentApi.createChat(
            workspace.id,
            request.modelConfigId,
            request.permissionMode,
            request.parameters,
          );
          session = created;
          id = created.id;
          memory.current.setDraft(id, session.repositoryPath, text);
          rememberCreatedChat(created);
          await refreshList().catch((cause: unknown) =>
            setError(agentError(cause)),
          );
        } catch (cause) {
          context.cancelChatCreatePreservation();
          throw cause;
        }
        if (selectionGeneration !== workspaceGeneration.current) {
          context.cancelChatCreatePreservation();
          throw new Error("创建期间工作区已切换；输入草稿已保留，请重试发送。");
        }
        await load(id, true, session.repositoryPath);
      }
      if (!session && record?.session.id === id) session = record.session;
      if (!session || session.id !== id)
        throw new Error("聊天配置尚未加载，请稍后重试。");
      if (!sessionWorkspace(session, workspaces))
        throw new Error(
          "此聊天的工作区已不可用；历史仍可阅读，请先选择有效工作区新建聊天。",
        );
      const currentModel = validateSelectedModel(configuration);
      const parameters = effectiveChatParameters(configuration, currentModel);
      if (
        Object.keys(
          modelParameterErrors(
            {
              id: currentModel.id,
              modelId: currentModel.modelId,
              label: currentModel.label,
              protocol: currentModel.protocol,
              enabled: currentModel.enabled,
              defaults: parameters,
              expectedRevision: currentModel.revision,
            },
            currentModel.capabilities,
          ),
        ).length > 0
      )
        throw new Error("聊天参数与模型能力冲突，请修正或恢复模型默认值。");
      if (!chatConfigurationMatchesSession(configuration, session)) {
        const configuredSession = await agentApi.configureChat({
          id,
          modelConfigId: configuration.modelConfigId,
          permissionMode: configuration.permissionMode,
          parameters: configuration.parameters,
        });
        session = configuredSession;
        setRecord((current) =>
          current?.session.id === id
            ? { ...current, session: configuredSession }
            : current,
        );
        setChatConfiguration(chatConfigurationForSession(configuredSession));
      }
      await agentApi.send(id, text, references.length ? references : undefined);
      context.clearReferences();
      const currentDraft = memory.current.draft(id, session.repositoryPath);
      const remainingDraft = draftAfterSend(currentDraft, text, true);
      if (remainingDraft !== currentDraft) {
        memory.current.setDraft(id, session.repositoryPath, remainingDraft);
        setDraft(remainingDraft);
      }
      if (sourceChatId === null && sourceScope)
        memory.current.setDraft(null, sourceScope, "");
      follow.current = true;
      await load(id);
      await refreshList();
    });
  }
  /** Enter 发送；修饰键、IME 组合和长按重复事件均不提交。 */
  function keyDown(event: KeyboardEvent<HTMLTextAreaElement>) {
    const result = handleComposerKeyDown(event, {
      enterHeld: enterHeld.current,
      canSend: run === null,
      send: () => {
        enterHeld.current = true;
        void send();
      },
      changeDraft: (value) => {
        memory.current.setDraft(selected.current, activeDraftScope(), value);
        setDraft(value);
      },
    });
    if (result === "send") enterHeld.current = true;
  }
  /** Enter 松开后允许下一次独立按键发送。 */
  function keyUp(event: KeyboardEvent<HTMLTextAreaElement>) {
    if (event.key === "Enter") enterHeld.current = false;
  }
  /** 输入框失焦时解除按住状态，避免切换焦点后无法再次发送。 */
  function composerBlur() {
    enterHeld.current = false;
  }
  /** 修改消息草稿，不同步到磁盘或浏览器存储。 */
  function changeDraft(event: ChangeEvent<HTMLTextAreaElement>) {
    memory.current.setDraft(
      selected.current,
      activeDraftScope(),
      event.target.value,
    );
    setDraft(event.target.value);
  }
  /** 修改标题草稿，由保存动作提交。 */
  function changeTitle(event: ChangeEvent<HTMLInputElement>) {
    setTitle(event.target.value);
  }
  /** 开始编辑指定聊天标题，在列表行菜单内保存。 */
  function beginRename(chat: ChatSession) {
    return () => {
      setRenameTarget(chat.id);
      setTitle(chat.title);
    };
  }
  /** 取消标题编辑，不修改聊天记录。 */
  function cancelRename() {
    setRenameTarget(null);
  }
  /** 标题经后端检查并在列表刷新后反映。 */
  function rename() {
    const chat = chats.find((item) => item.id === renameTarget);
    if (chat && !titleValidation(title).error)
      void action(async () => {
        await agentApi.update(chat.id, title, chat.archived);
        if (selected.current === chat.id) await load(chat.id);
        await refreshList();
        setRenameTarget(null);
      });
  }
  /** 归档不丢数据，恢复后可以继续聊天。 */
  function archive(chat: ChatSession) {
    return () => {
      void action(async () => {
        await agentApi.update(chat.id, chat.title, !chat.archived);
        if (selected.current === chat.id) await load(chat.id);
        await refreshList();
      });
    };
  }
  /** 在对应列表行打开删除确认。 */
  function askDelete(chat: ChatSession) {
    return () => setDeletePending(chat.id);
  }
  /** 撤销删除确认，不更改磁盘。 */
  function keepChat() {
    setDeletePending(null);
  }
  /** 删除后撤销选中引用，并刷新真实容量。 */
  function deleteChat() {
    const id = deletePending;
    if (id)
      void action(async () => {
        await agentApi.delete(id);
        memory.current.setDraft(id, chatRootPath(id), "");
        memory.current.endLoad(id);
        if (selected.current === id) {
          selected.current = null;
          setDraft(
            memory.current.draft(null, workspaceDraftScope(workspaceId)),
          );
          setChatConfiguration(null);
          generation.current++;
          setRecord(null);
          setRun(null);
        }
        setDeletePending(null);
        await refreshList();
        void settings.refreshUsage();
      });
  }
  /** 停止只阻止后续操作，在途 Git 写可能仍需等待。 */
  function cancel() {
    if (record && run)
      void action(() => agentApi.cancel(record.session.id, run.runId));
  }
  /** 批准只传运行和批准标识，不允许前端替换目标或参数。 */
  function resolve(approved: boolean) {
    return () => {
      if (record && run?.approval)
        void action(() =>
          agentApi.resolve(
            record.session.id,
            run.runId,
            run.approval!.id,
            approved,
          ),
        );
    };
  }
  /** 切换归档筛选不修改聊天自身状态。 */
  function toggleArchived() {
    setArchived((current) => !current);
  }
  /** 保存用户的滚动跟随意愿。 */
  function scroll(event: UIEvent<HTMLDivElement>) {
    const element = event.currentTarget;
    follow.current =
      element.scrollHeight - element.scrollTop - element.clientHeight < 90;
  }
  const selectedWorkspace =
    workspaces.find((item) => item.id === workspaceId) ?? null;
  const currentChatWorkspace = record
    ? sessionWorkspace(record.session, workspaces)
    : null;
  /** 判断历史会话的文件夹工作区是否仍是当前签发目录。 */
  function chatWorkspaceAvailable(chat: ChatSession): boolean {
    return sessionWorkspace(chat, workspaces) !== null;
  }
  const currentConfiguration = activeConfigurationDraft();
  const configurationDirty =
    record !== null &&
    !chatConfigurationMatchesSession(currentConfiguration, record.session);
  const contextModel = settings.view?.settings.models.find(
    (model) => model.id === currentConfiguration.modelConfigId,
  );
  const contextProvider = settings.view?.settings.providers.find(
    (provider) => provider.id === contextModel?.providerId,
  );
  const contextConfigurationKey = JSON.stringify([
    currentConfiguration.modelConfigId,
    contextModel?.revision ?? null,
    contextProvider?.revision ?? null,
    currentConfiguration.parameters,
    contextModel?.defaults ?? null,
    contextModel?.capabilities ?? null,
    record?.session.parameters ?? null,
    record?.session.memory ?? null,
  ]);
  const context = useAgentContext({
    enabled,
    workspaceId: record?.session.workspaceId ?? workspaceId,
    chatId: record?.session.id ?? null,
    text: draft,
    running: run !== null,
    ensureChat: ensureChatForMemory,
    configurationKey: contextConfigurationKey,
    configurationReady: !configurationDirty,
  });
  const search = useAgentSearch(enabled, workspaceId, archived);
  const availableModels = enabledModels();
  const recentModels = (settings.view?.recentModelConfigIds ?? [])
    .flatMap((id) => availableModels.find((model) => model.id === id) ?? [])
    .slice(0, 5);
  const recentModelIds = new Set(recentModels.map((model) => model.id));
  const otherModels = availableModels.filter(
    (model) => !recentModelIds.has(model.id),
  );
  const missingModel =
    currentConfiguration.modelConfigId !== null &&
    !enabledModels().some(
      (model) => model.id === currentConfiguration.modelConfigId,
    );
  const mismatched = record !== null && currentChatWorkspace === null;
  const disabled =
    !enabled ||
    configurationBusy ||
    (busy && run === null) ||
    record?.session.archived === true ||
    mismatched ||
    missingModel;
  return {
    settings,
    workspaces,
    workspaceId,
    selectedWorkspace,
    currentChatWorkspace,
    workspaceBusy,
    configurationBusy,
    configuration: currentConfiguration,
    configurationDirty,
    missingModel,
    enabledModels: availableModels,
    recentModels,
    otherModels,
    chatWorkspaceAvailable,
    chats: chats.filter((chat) => chat.archived === archived),
    unavailable,
    record,
    run,
    error,
    draft,
    title,
    titleValidation: titleValidation(title),
    renameTarget,
    busy,
    archived,
    deletePending,
    end,
    messagesRoot,
    search,
    context,
    selectSearchResult,
    disabled,
    mismatched,
    repositoryPath,
    select,
    create,
    send,
    keyDown,
    keyUp,
    composerBlur,
    changeDraft,
    changeTitle,
    beginRename,
    cancelRename,
    rename,
    archive,
    askDelete,
    keepChat,
    deleteChat,
    cancel,
    resolve,
    toggleArchived,
    scroll,
    selectWorkspace,
    chooseFolder,
    changeModelConfig,
    changePermissionMode,
    chatParameterEditor,
    saveChatConfiguration,
    ensureChatForMemory,
    updateChatSession,
  };
}
export type AgentController = ReturnType<typeof useAgent>;
