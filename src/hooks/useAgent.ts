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
  ChatView,
  RunNotice,
  RunSnapshot,
} from "../types/agent";
import { useAgentSettings } from "./useAgentSettings";
import { AgentChatMemory, titleValidation } from "../ui/agentChatMemory";
import { draftAfterSend, handleComposerKeyDown } from "../ui/agentComposer";
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
  const [renameTarget, setRenameTarget] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [archived, setArchived] = useState(false);
  const [deletePending, setDeletePending] = useState<string | null>(null);
  const selected = useRef<string | null>(null);
  const enterHeld = useRef(false);
  const memory = useRef(new AgentChatMemory());
  const project = useRef(repositoryPath);
  project.current = repositoryPath;
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
      setDraft(memory.current.draft(id, project.current));
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
  /** 没有聊天时按项目恢复草稿，切换项目不能带走未发送内容。 */
  useEffect(() => {
    if (selected.current === null)
      setDraft(memory.current.draft(null, repositoryPath));
  }, [repositoryPath]);
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
      const source = id;
      const sourceProject = project.current;
      const text = draft;
      if (!id) {
        const chat = await agentApi.create();
        if (chat.repositoryPath !== sourceProject)
          throw new Error("项目已切换，原项目草稿已保留，请返回原项目后重试。");
        id = chat.id;
        memory.current.setDraft(id, sourceProject, text);
        await load(id, true);
      }
      if (project.current !== sourceProject)
        throw new Error("项目已切换，草稿已保留，请检查聊天所属项目后重试。");
      await agentApi.send(id, text);
      const currentDraft = memory.current.draft(id, sourceProject);
      const remainingDraft = draftAfterSend(currentDraft, text, true);
      if (remainingDraft !== currentDraft) {
        memory.current.setDraft(id, sourceProject, remainingDraft);
        setDraft(remainingDraft);
      }
      if (source !== id) memory.current.setDraft(source, sourceProject, "");
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
        memory.current.setDraft(selected.current, project.current, value);
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
      project.current,
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
        memory.current.setDraft(id, project.current, "");
        memory.current.endLoad(id);
        if (selected.current === id) {
          selected.current = null;
          setDraft(memory.current.draft(null, project.current));
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
  const mismatched =
    record !== null && record.session.repositoryPath !== repositoryPath;
  const disabled =
    !enabled ||
    (busy && run === null) ||
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
    titleValidation: titleValidation(title),
    renameTarget,
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
  };
}
export type AgentController = ReturnType<typeof useAgent>;
