import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ChangeEvent,
  type KeyboardEvent,
} from "react";
import type { Workbench } from "./useWorkbench";
import type {
  AiEvent,
  AiEventEnvelope,
  AiJobSnapshot,
  AiModelRef,
  AiPermission,
  AiRunStatus,
  AiSession,
} from "../types/ai";
import * as api from "../services/ai";
import { chooseAiModel } from "../ui/aiModelSelection";
import { useAiModels } from "./useAiModels";
import { useNativeDialog } from "./useNativeDialog";
import { buildAiTurns } from "../ui/aiTurns";
import {
  aiFailureCode,
  aiRecoveryAction,
  aiRequestIssue,
  mergeAiHistory,
  type AiIssue,
  type AiHistoryState,
  type AiConnectionCheck,
} from "../ui/aiRecovery";

export interface AiRow {
  key: string;
  event: AiEvent;
  turn?: number | null;
  step?: number | null;
}
export interface AiQuestion {
  id: string;
  question: string;
  options: { label: string; description?: string }[];
  multiSelect: boolean;
}
/** 将增量合并到对应消息或工具行；终态不能被迟到的增量覆盖。 */
export function mergeAiRows(events: readonly AiEventEnvelope[]): AiRow[] {
  const rows = new Map<string, AiRow>();
  for (const envelope of [...events].sort((a, b) => a.sequence - b.sequence)) {
    const event = envelope.event;
    if (event.kind === "status" || event.kind === "job") continue;
    const key =
      event.kind === "assistant"
        ? `assistant-${event.messageId}`
        : event.kind === "tool"
          ? `tool-${event.toolCallId}`
          : event.kind === "approval"
            ? `approval-${event.approvalId}`
            : event.kind === "question"
              ? `question-${event.questionId}`
              : `event-${envelope.sequence}`;
    const old = rows.get(key)?.event;
    if (
      old?.kind === "assistant" &&
      old.final &&
      event.kind === "assistant" &&
      !event.final
    )
      continue;
    const value =
      event.kind === "tool" && old?.kind === "tool"
        ? { ...event, name: old.name, input: old.input }
        : event;
    rows.set(key, {
      key,
      event: value,
      turn: envelope.turn,
      step: envelope.step,
    });
  }
  return [...rows.values()].filter(
    ({ event }) =>
      !(
        (event.kind === "approval" || event.kind === "question") &&
        event.withdrawn
      ),
  );
}
/** AI 会话、表单、订阅与任务逻辑独立于展示组件。 */
export function useAiWorkspace(w: Workbench) {
  const models = useAiModels();
  const repositoryId = w.repo.repository?.repositoryId ?? null;
  const [sessions, setSessions] = useState<AiSession[]>([]);
  const [session, setSession] = useState<AiSession | null>(null);
  const [events, setEvents] = useState<AiEventEnvelope[]>([]);
  const [draft, setDraft] = useState("");
  const [status, setStatus] = useState<AiRunStatus>("idle");
  const [globalBusy, setGlobalBusy] = useState(false);
  const [issue, setIssue] = useState<AiIssue | null>(null);
  const [historyState, setHistoryState] = useState<AiHistoryState>("idle");
  const [recoveryNotice, setRecoveryNotice] = useState("");
  const [connectionCheck, setConnectionCheck] =
    useState<AiConnectionCheck | null>(null);
  const connectionGeneration = useRef(0);
  const recovering = useRef(false);
  const recoverRequested = useRef(false);
  const [jobsVersion, setJobsVersion] = useState(0);
  const [connectionBusy, setConnectionBusy] = useState(false);
  const jobsRetryRequested = useRef(false);
  const [connectionVersion, setConnectionVersion] = useState(0);
  const [jobs, setJobs] = useState<AiJobSnapshot[]>([]);
  const [terminalOpen, setTerminalOpen] = useState(false);
  const [modelChoice, setModelChoice] = useState<AiModelRef | null>(null);
  const [permissionChoice, setPermissionChoice] =
    useState<AiPermission>("workspace-write");
  const [fullAccess, setFullAccess] = useState(false);
  const [riskAcknowledged, setRiskAcknowledged] = useState(false);
  const [nextOffset, setNextOffset] = useState<number | null>(null);
  const [answers, setAnswers] = useState<Record<string, string>>({});
  const [selections, setSelections] = useState<Record<string, string[]>>({});
  const generation = useRef(0);
  const current = useRef(session);
  current.current = session;
  const sending = useRef(false);
  const draftVersion = useRef(0);
  const pendingMessage = useRef<{
    id: string;
    text: string;
    sessionId: string;
  } | null>(null);
  const previousBusy = useRef(false);
  const runId = useRef<string | null>(null);
  const seen = useRef(new Set<string>());
  const epoch = useRef<string | null>(null);
  const transcript = useRef<HTMLDivElement>(null);
  const following = useRef(true);
  const busy = status !== "idle" && status !== "failed";
  const rows = useMemo(() => mergeAiRows(events), [events]);
  const options =
    models.snapshot?.settings.providers.flatMap((provider) =>
      provider.models.map((model) => ({
        value: JSON.stringify([provider.providerId, model.modelId]),
        label: model.displayName,
        group: provider.displayName,
        model: {
          providerId: provider.providerId,
          modelId: model.modelId,
          reasoningEffort: null,
        } satisfies AiModelRef,
      })),
    ) ?? [];
  const chosen = chooseAiModel(
    modelChoice,
    session?.model,
    models.snapshot?.settings.defaultModel,
    options[0]?.model,
  );
  const modelValue = chosen
    ? JSON.stringify([chosen.providerId, chosen.modelId])
    : "";
  const validModel = options.some((option) => option.value === modelValue);
  const connectionKey = JSON.stringify([
    repositoryId,
    session?.sessionId,
    modelValue,
    models.snapshot?.settings.revision,
  ]);
  const connectionContext = useRef(connectionKey);
  connectionContext.current = connectionKey;
  useEffect(() => {
    connectionGeneration.current++;
    setConnectionCheck(null);
    setRecoveryNotice("");
    return () => {
      connectionGeneration.current++;
    };
  }, [connectionKey]);
  const efforts = models.snapshot?.settings.providers
    .find((provider) => provider.providerId === chosen?.providerId)
    ?.models.find(
      (model) => model.modelId === chosen?.modelId,
    )?.reasoningEfforts;
  /** 显式动作失败时保留原输入，错误不覆盖已显示的历史。 */
  function report(cause: unknown, scope: AiIssue["scope"] = "runtime"): void {
    setIssue({
      scope,
      code: aiFailureCode(cause),
      message: api.aiError(cause),
      title:
        scope === "history"
          ? "对话记录加载失败"
          : scope === "job"
            ? "后台任务状态读取失败"
            : scope === "model-request"
              ? "本次请求失败"
              : "会话操作未完成",
      sessionId: current.current?.sessionId ?? null,
      runtimeEpoch: epoch.current,
      runId: runId.current,
      turn: null,
    });
  }
  /** Promise 回调绑定调用时的代次，切换会话后不得回填旧错误。 */
  function reportTo(
    scope: AiIssue["scope"] = "runtime",
  ): (cause: unknown) => void {
    const token = generation.current;
    return (cause) => {
      if (token === generation.current) report(cause, scope);
    };
  }
  /** 收到事件先隔离运行时代次，再按序号去重。 */
  const accept = useCallback((event: AiEventEnvelope): void => {
    if (event.sessionId !== current.current?.sessionId) return;
    if (epoch.current && epoch.current !== event.runtimeEpoch) {
      seen.current.clear();
      setEvents([]);
    }
    epoch.current = event.runtimeEpoch;
    const key = `${event.runtimeEpoch}/${event.sequence}`;
    if (seen.current.has(key)) return;
    seen.current.add(key);
    setEvents((old) => [...old.slice(-5999), event]);
    if (event.runId) runId.current = event.runId;
    if (event.event.kind === "status") {
      setStatus(event.event.status);
      if (event.event.status === "idle") runId.current = null;
    }
  }, []);
  /** 会话列表分页只在当前项目仍匹配时应用。 */
  const reload = useCallback(
    async (offset = 0): Promise<boolean> => {
      if (!repositoryId || w.preview) return false;
      const token = generation.current;
      try {
        const page = await api.listAiSessions(repositoryId, offset);
        if (token !== generation.current) return false;
        setSessions((old) => (offset ? [...old, ...page.items] : page.items));
        setNextOffset(page.nextOffset);
        return true;
      } catch (cause) {
        if (token === generation.current) report(cause, "history");
        return false;
      }
    },
    [repositoryId, w.preview],
  );
  useEffect(() => {
    generation.current++;
    setSession(null);
    setSessions([]);
    setEvents([]);
    setStatus("idle");
    setJobs([]);
    setIssue(null);
    setHistoryState("idle");
    recovering.current = false;
    recoverRequested.current = false;
    jobsRetryRequested.current = false;
    setModelChoice(null);
    seen.current.clear();
  }, [repositoryId]);
  useEffect(() => {
    if (w.aiVisible) void reload();
  }, [w.aiVisible, reload]);
  useEffect(() => {
    if (!repositoryId || !session || w.preview) return;
    let disposed = false;
    let unsubscribe: (() => void) | undefined;
    const token = generation.current;
    let live: AiEventEnvelope[] = [];
    // 仅在恢复开始时捕获已显示内容；增量由 live 缓冲接续，不能随消息变化重启此副作用。
    const previousEvents = events;
    /** 先恢复并绑定项目，再订阅后补读，弥补历史读取与实时事件间的竞态。 */
    async function restore(): Promise<void> {
      recovering.current = true;
      setHistoryState("loading");
      for (let attempt = 0; attempt < 2; attempt++) {
        const first = await api.readAiSession(
          repositoryId!,
          session!.sessionId,
          0,
        );
        if (disposed || token !== generation.current) return;
        let history = first.events;
        let page = first;
        let cursor = first.nextSequence;
        live = [];
        let buffering = true;
        unsubscribe = await api.subscribeAiEvents(
          session!.sessionId,
          (event) => {
            if (disposed || token !== generation.current) return;
            if (buffering) live = [...live.slice(-5999), event];
            else accept(event);
          },
        );
        if (disposed || token !== generation.current) {
          unsubscribe();
          return;
        }
        do {
          page = await api.readAiSession(
            repositoryId!,
            session!.sessionId,
            cursor,
          );
          if (disposed || token !== generation.current) return;
          if (page.runtimeEpoch !== first.runtimeEpoch) break;
          history = mergeAiHistory(
            history,
            page.events,
            session!.sessionId,
            first.runtimeEpoch,
          );
          cursor = page.nextSequence;
        } while (page.more);
        if (
          page.runtimeEpoch !== first.runtimeEpoch ||
          live.some((event) => event.runtimeEpoch !== first.runtimeEpoch)
        ) {
          unsubscribe();
          if (attempt === 1) throw { code: "AI_RUNTIME_STOPPED" };
          continue;
        }
        const merged = mergeAiHistory(
          [...previousEvents, ...history],
          live,
          session!.sessionId,
          first.runtimeEpoch,
        );
        epoch.current = first.runtimeEpoch;
        seen.current = new Set(
          merged.map((event) => `${event.runtimeEpoch}/${event.sequence}`),
        );
        setEvents(merged);
        const latestStatus = [...live]
          .reverse()
          .find((event) => event.event.kind === "status");
        const restoredStatus =
          latestStatus?.event.kind === "status"
            ? latestStatus.event.status
            : page.status;
        runId.current =
          restoredStatus === "idle"
            ? null
            : (latestStatus?.runId ?? page.activeRunId);
        setStatus(restoredStatus);
        buffering = false;
        setHistoryState("ready");
        setIssue((old) =>
          old?.scope === "history" || old?.scope === "runtime" ? null : old,
        );
        if (recoverRequested.current)
          setRecoveryNotice("对话记录已重新加载，任务不会重新执行。");
        recoverRequested.current = false;
        recovering.current = false;
        return;
      }
    }
    void restore().catch((cause: unknown) => {
      if (!disposed && token === generation.current) {
        unsubscribe?.();
        recovering.current = false;
        setHistoryState("failed");
        report(cause, "history");
      }
    });
    return () => {
      disposed = true;
      unsubscribe?.();
    };
  }, [repositoryId, session?.sessionId, w.preview, accept, connectionVersion]);
  useEffect(() => {
    if (!session || w.preview || (!w.aiVisible && !globalBusy)) return;
    let active = true;
    let inFlight = false;
    /** 隐藏且空闲时停止读取任务，输出变化时才更新展示状态。 */
    function poll(): void {
      if (!active || inFlight) return;
      inFlight = true;
      void api
        .readAiJobs(session!.sessionId)
        .then((value) => {
          if (active) {
            setIssue((old) => (old?.scope === "job" ? null : old));
            if (jobsRetryRequested.current)
              setRecoveryNotice("后台任务状态已更新。");
            jobsRetryRequested.current = false;
            setJobs((old) =>
              old.length === value.length &&
              old.every(
                (job, index) =>
                  job.jobId === value[index]?.jobId &&
                  job.state === value[index]?.state &&
                  job.output === value[index]?.output,
              )
                ? old
                : value,
            );
          }
        })
        .catch((cause: unknown) => {
          if (active) {
            jobsRetryRequested.current = false;
            setRecoveryNotice("");
            report(cause, "job");
          }
          active = false;
        })
        .finally(() => {
          inFlight = false;
        });
    }
    poll();
    const timer = setInterval(poll, 700);
    return () => {
      active = false;
      clearInterval(timer);
    };
  }, [
    session?.sessionId,
    w.preview,
    w.aiVisible,
    globalBusy,
    connectionVersion,
    jobsVersion,
  ]);
  useEffect(() => {
    if (following.current && transcript.current)
      transcript.current.scrollTop = transcript.current.scrollHeight;
  }, [events]);
  useEffect(() => {
    if (w.preview || (!w.aiVisible && !globalBusy)) return;
    let active = true;
    let inFlight = false;
    const timer = setInterval(() => {
      if (!active || inFlight) return;
      inFlight = true;
      void api
        .readAiActivity()
        .then((value) => {
          if (active && !sending.current) {
            setGlobalBusy(value.busy);
            if (!value.busy) {
              setStatus("idle");
              runId.current = null;
            }
          }
        })
        .catch((cause: unknown) => {
          if (active) report(cause);
        })
        .finally(() => {
          inFlight = false;
        });
    }, 700);
    return () => {
      active = false;
      clearInterval(timer);
    };
  }, [
    w.preview,
    w.aiVisible,
    globalBusy,
    repositoryId,
    session?.sessionId,
    connectionVersion,
  ]);
  useEffect(() => {
    w.setAiRunning(globalBusy);
  }, [globalBusy, w.setAiRunning]);
  useEffect(() => {
    const finished = previousBusy.current && !globalBusy;
    previousBusy.current = globalBusy;
    if (finished && repositoryId) void w.refreshLocal().catch(reportTo());
  }, [globalBusy, repositoryId, w.refreshLocal]);
  useEffect(() => {
    if (!session && models.snapshot)
      setPermissionChoice(models.snapshot.settings.defaultPermission);
  }, [models.snapshot, session]);
  /** 选择旧会话只恢复，不发送模型请求。 */
  function select(item: AiSession, preserveGeneration = false): () => void {
    return () => {
      if (
        !preserveGeneration &&
        current.current?.sessionId === item.sessionId
      ) {
        reconnect();
        return;
      }
      if (!preserveGeneration) generation.current++;
      current.current = item;
      setSession(item);
      setModelChoice(item.model);
      setPermissionChoice(item.permission);
      setEvents([]);
      setJobs([]);
      seen.current.clear();
      epoch.current = null;
      setIssue(null);
      setHistoryState("loading");
      recovering.current = false;
      recoverRequested.current = false;
      jobsRetryRequested.current = false;
      setRecoveryNotice("");
      runId.current = null;
    };
  }
  /** 新会话只有真实模型可用时才可创建。 */
  async function create(): Promise<void> {
    if (
      !repositoryId ||
      !chosen ||
      !validModel ||
      busy ||
      historyState === "loading"
    )
      return;
    const token = generation.current;
    try {
      const value = await api.createAiSession(
        repositoryId,
        chosen,
        permissionChoice,
      );
      if (token !== generation.current) return;
      select(value)();
      await reload();
    } catch (cause) {
      if (token === generation.current) report(cause);
    }
  }
  /** 输入草稿只保存在内存中，不写 localStorage。 */
  function changeDraft(event: ChangeEvent<HTMLTextAreaElement>): void {
    draftVersion.current++;
    setDraft(event.target.value);
  }
  /** 输入法开始组合也算新草稿，迟到的发送确认不能清掉正在组合的文字。 */
  function composeDraft(): void {
    draftVersion.current++;
  }
  /** 一个任务固定一个消息 ID，失败时仍保留输入以供核对。 */
  async function send(): Promise<void> {
    if (
      !repositoryId ||
      !chosen ||
      !draft.trim() ||
      busy ||
      historyState === "loading" ||
      connectionBusy ||
      !validModel ||
      sending.current
    )
      return;
    sending.current = true;
    const token = generation.current;
    const message = draft;
    const messageVersion = draftVersion.current;
    setStatus("running");
    setGlobalBusy(true);
    setIssue(null);
    setRecoveryNotice("");
    try {
      let target = session;
      if (!target) {
        target = await api.createAiSession(
          repositoryId,
          chosen,
          permissionChoice,
        );
        if (token !== generation.current) return;
        select(target, true)();
      }
      if (token !== generation.current) return;
      const previous = pendingMessage.current;
      const clientMessageId =
        previous?.sessionId === target.sessionId && previous.text === message
          ? previous.id
          : crypto.randomUUID();
      pendingMessage.current = {
        id: clientMessageId,
        text: message,
        sessionId: target.sessionId,
      };
      const result = await api.sendAiMessage(
        repositoryId,
        target.sessionId,
        clientMessageId,
        message,
        w.editor.dirty || Boolean(w.editor.savingPath),
      );
      pendingMessage.current = null;
      if (token !== generation.current) return;
      runId.current = result.runId;
      if (draftVersion.current === messageVersion) {
        draftVersion.current++;
        setDraft("");
      }
      void reload();
    } catch (cause) {
      if (token === generation.current) {
        setStatus("idle");
        report(cause, "model-request");
      }
    } finally {
      sending.current = false;
    }
  }
  /** 中文输入法组合期间 Enter 不发送。 */
  function composerKey(event: KeyboardEvent<HTMLTextAreaElement>): void {
    if (
      event.key === "Enter" &&
      !event.shiftKey &&
      !event.nativeEvent.isComposing
    ) {
      event.preventDefault();
      void send();
    }
  }
  /** 停止生成保留后台任务，用户可在任务面板单独停止。 */
  async function stop(): Promise<void> {
    if (!session || !runId.current || status === "stopping") return;
    const onError = reportTo();
    setStatus("stopping");
    try {
      await api.cancelAiRun(session.sessionId, runId.current);
    } catch (cause) {
      onError(cause);
    }
  }
  /** 模型切换在会话空闲时应用，同时保存新会话默认值。 */
  async function chooseModel(value: string): Promise<void> {
    const choice = options.find((option) => option.value === value);
    if (!choice || busy || historyState === "loading" || !models.snapshot)
      return;
    const token = generation.current;
    try {
      const updated = session
        ? await api.updateAiSession(session.sessionId, { model: choice.model })
        : null;
      if (token !== generation.current) return;
      if (updated) setSession(updated);
      setModelChoice(choice.model);
      await api.updateAiDefaults(
        models.snapshot.settings.revision,
        choice.model,
        permissionChoice,
      );
      window.dispatchEvent(new Event("gitmaster:ai-settings"));
    } catch (cause) {
      if (token === generation.current) report(cause);
    }
  }
  /** 推理选择只接受当前模型已配置的档位。 */
  async function chooseEffort(
    event: ChangeEvent<HTMLSelectElement>,
  ): Promise<void> {
    if (!chosen || busy || historyState === "loading" || !models.snapshot)
      return;
    const value = event.target.value;
    if (value && !Object.hasOwn(efforts ?? {}, value)) return;
    const model = { ...chosen, reasoningEffort: value || null };
    const token = generation.current;
    try {
      const updated = session
        ? await api.updateAiSession(session.sessionId, { model })
        : null;
      if (token !== generation.current) return;
      if (updated) setSession(updated);
      setModelChoice(model);
      await api.updateAiDefaults(
        models.snapshot.settings.revision,
        model,
        permissionChoice,
      );
      window.dispatchEvent(new Event("gitmaster:ai-settings"));
    } catch (cause) {
      if (token === generation.current) report(cause);
    }
  }
  /** 真正写入权限选择前处理后台任务与完全访问确认。 */
  async function applyPermission(permission: AiPermission): Promise<void> {
    const token = generation.current;
    try {
      const updated = session
        ? await api.updateAiSession(session.sessionId, { permission })
        : null;
      if (token !== generation.current) return;
      if (updated) setSession(updated);
      setPermissionChoice(permission);
      if (models.snapshot) {
        await api.updateAiDefaults(
          models.snapshot.settings.revision,
          chosen,
          permission,
        );
        window.dispatchEvent(new Event("gitmaster:ai-settings"));
      }
    } catch (cause) {
      if (token === generation.current) report(cause);
    }
  }
  /** 仅接受固定权限选项，不把任意字符串传给后端。 */
  function choosePermission(value: AiPermission): void {
    if (busy || historyState === "loading") return;
    if (value === "danger-full-access") setFullAccess(true);
    else void applyPermission(value);
  }
  /** 关闭确认不改变当前权限。 */
  function cancelFullAccess(): void {
    setFullAccess(false);
    setRiskAcknowledged(false);
  }
  /** 风险确认必须来自用户主动勾选。 */
  function acknowledgeRisk(event: ChangeEvent<HTMLInputElement>): void {
    setRiskAcknowledged(event.target.checked);
  }
  /** 明确确认后才切换到完全访问。 */
  function confirmFullAccess(): void {
    if (!riskAcknowledged) return;
    cancelFullAccess();
    void applyPermission("danger-full-access");
  }
  const permissionDialog = useNativeDialog(fullAccess, cancelFullAccess);
  /** 审批只对应一个未过期请求。 */
  function approve(
    id: string,
    decision: "allowed-once" | "rejected",
  ): () => void {
    return () => {
      if (session)
        void api
          .resolveAiApproval(session.sessionId, id, decision)
          .catch(reportTo());
    };
  }
  /** 每个问题保留独立的文本回答，不转换为权限授权。 */
  function answerField(
    requestId: string,
    question: AiQuestion,
  ): (event: ChangeEvent<HTMLInputElement>) => void {
    return (event) => {
      const key = `${requestId}/${question.id}`;
      setAnswers((old) => ({ ...old, [key]: event.target.value }));
      if (!question.multiSelect)
        setSelections((old) => ({ ...old, [key]: [] }));
    };
  }
  /** 选项按所属请求隔离，单选和多选分别保留原始标签。 */
  function selectAnswer(
    requestId: string,
    question: AiQuestion,
    label: string,
  ): () => void {
    return () => {
      const key = `${requestId}/${question.id}`;
      setSelections((old) => ({
        ...old,
        [key]: question.multiSelect
          ? old[key]?.includes(label)
            ? old[key].filter((item) => item !== label)
            : [...(old[key] ?? []), label]
          : [label],
      }));
      if (!question.multiSelect) setAnswers((old) => ({ ...old, [key]: "" }));
    };
  }
  /** 问题元数据只用于展示和原始 ID 回填。 */
  function questions(detail: string): AiQuestion[] {
    try {
      const value: unknown = JSON.parse(detail);
      if (!Array.isArray(value)) return [];
      return value.flatMap((item: unknown) =>
        item &&
        typeof item === "object" &&
        "id" in item &&
        typeof item.id === "string" &&
        "question" in item &&
        typeof item.question === "string"
          ? [
              {
                id: item.id,
                question: item.question,
                multiSelect: "multiSelect" in item && item.multiSelect === true,
                options:
                  "options" in item && Array.isArray(item.options)
                    ? item.options.flatMap((option: unknown) =>
                        option &&
                        typeof option === "object" &&
                        "label" in option &&
                        typeof option.label === "string"
                          ? [
                              {
                                label: option.label,
                                description:
                                  "description" in option &&
                                  typeof option.description === "string"
                                    ? option.description
                                    : undefined,
                              },
                            ]
                          : [],
                      )
                    : [],
              },
            ]
          : [],
      );
    } catch {
      return [];
    }
  }
  /** 空答案是跳过，不是默认批准。 */
  function answer(id: string, detail: string): () => void {
    return () => {
      if (session)
        void api
          .answerAiQuestion(session.sessionId, id, {
            answers: questions(detail).map((question) => ({
              id: question.id,
              selected: selections[`${id}/${question.id}`] ?? [],
              custom: answers[`${id}/${question.id}`] ?? "",
            })),
          })
          .catch(reportTo());
    };
  }
  /** 任务停止后仍由真实快照确认结束状态。 */
  function stopJob(jobId: string): () => void {
    return () => {
      if (session)
        void api.stopAiJob(session.sessionId, jobId).catch(reportTo());
    };
  }
  /** 全部停止仍逐一绑定当前会话，结果由后续真实任务快照确认。 */
  function stopAllJobs(): void {
    if (session)
      void Promise.all(
        jobs
          .filter((job) => job.state === "running")
          .map((job) => api.stopAiJob(session.sessionId, job.jobId)),
      ).catch(reportTo());
  }
  /** 删除包含运行任务的会话必须明确确认。 */
  function remove(item: AiSession): () => void {
    return () => {
      if (
        !window.confirm(
          `删除“${item.title}”？此会话的任务会结束，项目文件会保留。`,
        )
      )
        return;
      const token = generation.current;
      void api
        .deleteAiSession(item.sessionId, true)
        .then(() => {
          if (token !== generation.current) return;
          if (current.current?.sessionId === item.sessionId) {
            setSession(null);
            setEvents([]);
            setStatus("idle");
          }
          void reload();
        })
        .catch(reportTo());
    };
  }
  /** 重命名仅改变应用会话标题。 */
  function renameSession(): void {
    if (!session) return;
    const token = generation.current;
    const title = window.prompt("会话名称", session.title)?.trim();
    if (title)
      void api
        .updateAiSession(session.sessionId, { title })
        .then((value) => {
          if (token === generation.current) setSession(value);
        })
        .then(() => reload())
        .catch(reportTo());
  }
  /** 只有空闲会话可触发手动上下文压缩。 */
  function compact(): void {
    if (session && !busy) {
      const token = generation.current;
      setStatus("running");
      void api
        .compactAiSession(session.sessionId)
        .then((result) => {
          if (token !== generation.current) return;
          runId.current = result.runId;
          setGlobalBusy(true);
        })
        .catch((cause: unknown) => {
          if (token !== generation.current) return;
          setStatus("failed");
          report(cause);
        });
    }
  }
  /** 用户主动向上阅读时暂停自动跟随。 */
  function scroll(): void {
    const element = transcript.current;
    if (element)
      following.current =
        element.scrollHeight - element.scrollTop - element.clientHeight < 80;
  }
  /** 打开模型设置复用现有设置外壳。 */
  function settings(): void {
    w.selectSettings("ai")();
    w.openModal("settings")();
  }
  /** 任务面板折叠不结束任何进程。 */
  function toggleTerminal(): void {
    setTerminalOpen((old) => !old);
  }
  /** 加载下一页会话，不丢弃已有列表。 */
  function more(): void {
    if (nextOffset !== null) void reload(nextOffset);
  }
  /** 手动重建历史订阅，不清除输入，也不重发先前任务。 */
  function reconnect(): void {
    if (sending.current || recovering.current) return;
    generation.current++;
    recoverRequested.current = true;
    setIssue(null);
    setRecoveryNotice("");
    setHistoryState("loading");
    setConnectionVersion((old) => old + 1);
    if (!session) {
      recovering.current = true;
      const token = generation.current;
      void reload().then((ok) => {
        if (token !== generation.current) return;
        recovering.current = false;
        setHistoryState(ok ? "ready" : "failed");
        if (ok) setRecoveryNotice("会话列表已重新加载。");
      });
    }
  }
  /** 主动测试当前模型，既不重发历史输入，也不触发任何工具。 */
  async function testConnection(): Promise<void> {
    if (!chosen || !validModel || connectionBusy || busy || globalBusy) return;
    const token = ++connectionGeneration.current;
    const key = connectionKey;
    setConnectionBusy(true);
    setRecoveryNotice("");
    setConnectionCheck({ state: "testing", key, checkedAt: null, error: null });
    try {
      const result = await api.testAiProvider(
        chosen.providerId,
        chosen.modelId,
      );
      if (
        token !== connectionGeneration.current ||
        connectionContext.current !== key
      )
        return;
      setConnectionCheck({
        state: result.ok ? "succeeded" : "failed",
        key,
        checkedAt: Date.now(),
        error: result.ok ? null : result.error,
      });
    } catch (cause) {
      if (
        token === connectionGeneration.current &&
        connectionContext.current === key
      )
        setConnectionCheck({
          state: "failed",
          key,
          checkedAt: Date.now(),
          error: { code: aiFailureCode(cause), message: api.aiError(cause) },
        });
    } finally {
      setConnectionBusy(false);
    }
  }
  /** 仅重新启动任务快照读取，不重载对话或重新执行后台命令。 */
  function retryJobs(): void {
    jobsRetryRequested.current = true;
    setIssue((old) => (old?.scope === "job" ? null : old));
    setJobsVersion((old) => old + 1);
    setRecoveryNotice("正在重新读取后台任务状态…");
  }
  const recovery =
    issue ??
    (historyState === "ready" ? aiRequestIssue(events, runId.current) : null);
  const recoveryAction = recovery ? aiRecoveryAction(recovery) : null;
  const recoveryLabel =
    recoveryAction === "connect"
      ? connectionBusy
        ? "正在测试…"
        : "测试模型连接"
      : recoveryAction === "settings"
        ? "打开模型设置"
        : recoveryAction === "jobs"
          ? "重新读取状态"
          : historyState === "loading"
            ? "正在加载…"
            : recovery?.scope === "history"
              ? "重新加载记录"
              : "重新连接会话";
  /** 恢复动作按错误来源分派，始终不重发任务或命令。 */
  function recover(): void {
    if (recoveryAction === "connect") void testConnection();
    else if (recoveryAction === "settings") settings();
    else if (recoveryAction === "jobs") retryJobs();
    else if (recoveryAction === "reload") reconnect();
  }
  const visibleCheck =
    connectionCheck?.key === connectionKey ? connectionCheck : null;
  const connectionDetail =
    visibleCheck?.state === "testing"
      ? "正在测试所选模型连接…"
      : visibleCheck?.state === "succeeded"
        ? "本次模型连接测试通过；后续请求和工具以实际运行结果为准。"
        : visibleCheck?.state === "failed"
          ? (visibleCheck.error?.message ?? "本次模型连接测试失败。")
          : "";
  return {
    presentation: w.appSettings.draft.uiPreferences.aiChat,
    turns: buildAiTurns(rows, w.appSettings.draft.uiPreferences.aiChat, busy),
    sessions,
    session,
    rows,
    draft,
    status,
    busy,
    error: recovery?.message ?? null,
    recovery,
    recoveryAction,
    recoveryLabel,
    recover,
    recoveryDisabled:
      recoveryAction === "connect"
        ? connectionBusy || busy || globalBusy || !validModel
        : recoveryAction === "reload"
          ? historyState === "loading" || sending.current
          : false,
    historyState,
    recoveryNotice,
    connectionCheck: visibleCheck,
    connectionDetail,
    connectionTime: visibleCheck?.checkedAt
      ? new Date(visibleCheck.checkedAt).toLocaleTimeString()
      : "",
    connectionBusy,
    testConnection,
    retryJobs,
    jobs,
    terminalOpen,
    modelValue,
    validModel,
    options,
    permissionChoice,
    efforts,
    reasoningEffort: chosen?.reasoningEffort ?? "",
    models,
    repositoryId,
    preview: w.preview,
    transcript,
    permissionDialog,
    fullAccess,
    riskAcknowledged,
    acknowledgeRisk,
    answers,
    selections,
    selectAnswer,
    questions,
    answerField,
    answer,
    select,
    create,
    changeDraft,
    composeDraft,
    send,
    composerKey,
    stop,
    chooseModel,
    chooseEffort,
    choosePermission,
    cancelFullAccess,
    confirmFullAccess,
    approve,
    stopJob,
    stopAllJobs,
    remove,
    renameSession,
    compact,
    scroll,
    settings,
    toggleTerminal,
    more,
    reconnect,
    nextOffset,
  };
}
export type AiWorkspaceController = ReturnType<typeof useAiWorkspace>;
