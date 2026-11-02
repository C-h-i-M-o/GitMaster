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
} from "../services/agent";
import type {
  ChatRecord,
  ChatSession,
  RunNotice,
  RunSnapshot,
} from "../types/agent";
import { useAgentSettings } from "./useAgentSettings";
/** 管理本地聊天与运行事件；不持久化前端副本或密钥。 */
export function useAgent(
  enabled: boolean,
  repositoryPath: string | null,
  refreshRepository: () => Promise<void>,
) {
  const settings = useAgentSettings(enabled);
  const [chats, setChats] = useState<ChatSession[]>([]);
  const [unavailable, setUnavailable] = useState<string[]>([]);
  const [record, setRecord] = useState<ChatRecord | null>(null);
  const [run, setRun] = useState<RunSnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [draft, setDraft] = useState("");
  const [title, setTitle] = useState("");
  const [busy, setBusy] = useState(false);
  const [archived, setArchived] = useState(false);
  const [deletePending, setDeletePending] = useState(false);
  const selected = useRef<string | null>(null);
  const generation = useRef(0);
  const progressVersion = useRef(0);
  const finished = useRef(new Set<string>());
  const end = useRef<HTMLDivElement | null>(null);
  const follow = useRef(true);
  const refreshRef = useRef(refreshRepository);
  refreshRef.current = refreshRepository;
  /** 列表独立刷新，不覆盖用户正在阅读的聊天。 */
  const refreshList = useCallback(async () => {
    const result = await agentApi.chats();
    setChats(result.chats.sort((a, b) => b.updatedAt - a.updatedAt));
    setUnavailable(result.unavailable);
  }, []);
  /** 用请求代次隔离快速切换；活动通知比读取开始后的旧快照更新。 */
  const load = useCallback(async (id: string, switching = false) => {
    if (switching) {
      selected.current = id;
      setRecord(null);
      setRun(null);
      setTitle("");
      setDeletePending(false);
      follow.current = true;
    }
    const version = ++generation.current;
    const progress = progressVersion.current;
    const view = await agentApi.read(id);
    if (selected.current !== id || version !== generation.current) return;
    setRecord((current) => {
      if (current?.session.id !== id) return view.record;
      const events = new Map(
        view.record.events.map((event) => [event.sequence, event]),
      );
      for (const event of current.events) events.set(event.sequence, event);
      return {
        ...view.record,
        events: [...events.values()].sort((a, b) => a.sequence - b.sequence),
      };
    });
    if (switching) setTitle(view.record.session.title);
    if (progress === progressVersion.current)
      setRun(
        view.run && !finished.current.has(view.run.runId) ? view.run : null,
      );
  }, []);
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
    [load, refreshList],
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
    return () => {
      disposed = true;
      cleanup.forEach((stop) => stop());
    };
  }, [enabled, receive, refreshList]);
  /** 阅读历史时不强行滚动，只有接近底部时跟随新消息。 */
  useEffect(() => {
    if (follow.current) end.current?.scrollIntoView({ block: "end" });
  }, [record?.events.length, run?.text, run?.phase]);
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
      void action(() => load(chat.id, true));
    };
  }
  /** 新建聊天绑定桌面当前仓库；无配置先打开首配。 */
  function create() {
    if (!settings.view?.settings.activeProfileId) {
      settings.add();
      return;
    }
    void action(async () => {
      const chat = await agentApi.create();
      await refreshList();
      await load(chat.id, true);
      setDraft("");
    });
  }
  /** 发送时才创建缺失聊天，输入在成功接受后清空。 */
  async function send(event?: FormEvent) {
    event?.preventDefault();
    if (!draft.trim() || run || busy) return;
    if (!settings.view?.settings.activeProfileId) {
      settings.add();
      return;
    }
    await action(async () => {
      let id = selected.current;
      if (!id) {
        const chat = await agentApi.create();
        id = chat.id;
        await load(id, true);
      }
      await agentApi.send(id, draft);
      setDraft("");
      follow.current = true;
      await load(id);
      await refreshList();
    });
  }
  /** 输入法组合期间不发送；Ctrl/Cmd+Enter 提交。 */
  function keyDown(event: KeyboardEvent<HTMLTextAreaElement>) {
    if (
      event.key === "Enter" &&
      (event.ctrlKey || event.metaKey) &&
      !event.nativeEvent.isComposing
    ) {
      event.preventDefault();
      void send();
    }
  }
  /** 修改消息草稿，不同步到磁盘或浏览器存储。 */
  function changeDraft(event: ChangeEvent<HTMLTextAreaElement>) {
    setDraft(event.target.value);
  }
  /** 修改标题草稿，由保存动作提交。 */
  function changeTitle(event: ChangeEvent<HTMLInputElement>) {
    setTitle(event.target.value);
  }
  /** 标题和归档均经后端检查运行状态。 */
  function rename() {
    if (record)
      void action(async () => {
        await agentApi.update(
          record.session.id,
          title,
          record.session.archived,
        );
        await load(record.session.id);
        await refreshList();
      });
  }
  /** 归档不丢数据，恢复后可以继续聊天。 */
  function archive() {
    if (record)
      void action(async () => {
        await agentApi.update(
          record.session.id,
          record.session.title,
          !record.session.archived,
        );
        await load(record.session.id);
        await refreshList();
      });
  }
  /** 两步确认删除真实本地记录。 */
  function askDelete() {
    setDeletePending(true);
  }
  /** 撤销删除确认，不更改磁盘。 */
  function keepChat() {
    setDeletePending(false);
  }
  /** 删除后撤销选中引用，并刷新真实容量。 */
  function deleteChat() {
    if (record)
      void action(async () => {
        await agentApi.delete(record.session.id);
        selected.current = null;
        generation.current++;
        setRecord(null);
        setRun(null);
        setDeletePending(false);
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
  const mismatched =
    record !== null && record.session.repositoryPath !== repositoryPath;
  const disabled =
    !enabled ||
    busy ||
    run !== null ||
    record?.session.archived === true ||
    mismatched;
  return {
    settings,
    chats: chats.filter((chat) => chat.archived === archived),
    unavailable,
    record,
    run,
    error,
    draft,
    title,
    busy,
    archived,
    deletePending,
    end,
    disabled,
    mismatched,
    repositoryPath,
    select,
    create,
    send,
    keyDown,
    changeDraft,
    changeTitle,
    rename,
    archive,
    askDelete,
    keepChat,
    deleteChat,
    cancel,
    resolve,
    toggleArchived,
    scroll,
  };
}
export type AgentController = ReturnType<typeof useAgent>;
