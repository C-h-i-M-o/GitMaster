import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type ChangeEvent,
} from "react";
import { agentApi, agentError } from "../services/agent";
import type { ChatSession } from "../types/agent";
import type {
  ContextPreviewResult,
  UserReferenceSpec,
  VerifiedReference,
} from "../types/agentContext";
import {
  agentContextRequestKey,
  bindReferenceVersions,
  contextScopeTransition,
  referenceBatchError,
} from "../ui/agentContext";

interface ScopedPreview {
  key: string;
  result: ContextPreviewResult;
}

/** 管理用户引用、版本校验和聊天上下文估算，隔离切换后的迟到结果。 */
export function useAgentContext({
  enabled,
  workspaceId,
  chatId,
  text,
  running,
  ensureChat,
  configurationKey,
  configurationReady,
}: {
  enabled: boolean;
  workspaceId: string;
  chatId: string | null;
  text: string;
  running: boolean;
  ensureChat: () => Promise<ChatSession>;
  configurationKey: string;
  configurationReady: boolean;
}) {
  const [open, setOpen] = useState(false);
  const [references, setReferences] = useState<UserReferenceSpec[]>([]);
  const [verified, setVerified] = useState<VerifiedReference[]>([]);
  const [referenceBusy, setReferenceBusy] = useState(false);
  const [referenceError, setReferenceError] = useState<string | null>(null);
  const [path, setPath] = useState("");
  const [startLine, setStartLine] = useState("");
  const [endLine, setEndLine] = useState("");
  const [textSelection, setTextSelection] = useState("");
  const [staged, setStaged] = useState(false);
  const [previewRequested, setPreviewRequested] = useState(false);
  const [contextBusy, setContextBusy] = useState(false);
  const [contextError, setContextError] = useState<string | null>(null);
  const [contextState, setContextState] = useState<ScopedPreview | null>(null);
  const [memoryRevision, setMemoryRevision] = useState(0);
  const [previewChatId, setPreviewChatId] = useState<string | null>(null);
  const [preparingChat, setPreparingChat] = useState(false);
  const referenceGeneration = useRef(0);
  const contextGeneration = useRef(0);
  const ensurePending = useRef(false);
  const preserveReferencesOnCreate = useRef(false);
  const previousScope = useRef({ workspaceId, chatId });
  const latestWorkspace = useRef(workspaceId);
  latestWorkspace.current = workspaceId;
  const effectiveChatId = chatId ?? previewChatId;
  const previewKey = agentContextRequestKey(
    workspaceId,
    effectiveChatId,
    text,
    references,
    configurationKey,
    memoryRevision,
  );

  /** 切换聊天或工作区后清除引用，并使所有在途预览失效。 */
  useEffect(() => {
    const previous = previousScope.current;
    if (previous.workspaceId !== workspaceId || previous.chatId !== chatId) {
      const transition = contextScopeTransition(
        previous,
        { workspaceId, chatId },
        preserveReferencesOnCreate.current,
      );
      referenceGeneration.current += 1;
      contextGeneration.current += 1;
      if (transition.clearReferenceBusy) setReferenceBusy(false);
      if (transition.clearContextBusy) setContextBusy(false);
      if (!transition.preserveReferences) {
        setReferences([]);
        setVerified([]);
      }
      setReferenceError(null);
      setContextError(null);
      setContextState(null);
      if (!transition.preserveReferences) {
        setPreviewRequested(false);
        setPreviewChatId(null);
      }
      preserveReferencesOnCreate.current = false;
      previousScope.current = { workspaceId, chatId };
    }
  }, [workspaceId, chatId]);

  /** 对每个草稿版本延迟预览，旧查询迟到时不覆盖最新结果。 */
  useEffect(() => {
    const request = ++contextGeneration.current;
    if (
      !open ||
      !previewRequested ||
      !enabled ||
      !configurationReady ||
      !effectiveChatId ||
      !text.trim()
    ) {
      setContextBusy(false);
      return () => {
        contextGeneration.current += 1;
      };
    }
    if (referenceBusy) {
      setContextState(null);
      setContextError(null);
      setContextBusy(false);
      return () => {
        contextGeneration.current += 1;
      };
    }
    const invalid = referenceBatchError(references);
    if (invalid || references.some((reference) => !reference.expectedVersion)) {
      setContextState(null);
      setContextError(
        invalid ?? (references.length ? "请先校验所选引用。" : null),
      );
      setContextBusy(false);
      return () => {
        contextGeneration.current += 1;
      };
    }
    setContextState(null);
    setContextError(null);
    setContextBusy(true);
    const timer = window.setTimeout(() => {
      void agentApi
        .previewContext(
          effectiveChatId,
          text,
          references.length ? references : undefined,
        )
        .then((result) => {
          if (contextGeneration.current === request)
            setContextState({ key: previewKey, result });
        })
        .catch((cause: unknown) => {
          if (contextGeneration.current === request)
            setContextError(agentError(cause));
        })
        .finally(() => {
          if (contextGeneration.current === request) setContextBusy(false);
        });
    }, 280);
    return () => {
      window.clearTimeout(timer);
      contextGeneration.current += 1;
    };
  }, [
    open,
    previewRequested,
    enabled,
    configurationReady,
    workspaceId,
    effectiveChatId,
    text,
    references,
    configurationKey,
    memoryRevision,
    referenceBusy,
    previewKey,
  ]);

  /** 显示引用及上下文入口。 */
  function show(): void {
    setOpen(true);
  }

  /** 关闭面板时停止显示旧预览；引用保留到切换聊天或工作区。 */
  function close(): void {
    setOpen(false);
    setContextState(null);
    setPreviewRequested(false);
  }

  /** 将单个引用加入集合并请求后端重新读取、校验版本。 */
  function addReference(reference: UserReferenceSpec): boolean {
    if (running || referenceBusy || !enabled) return false;
    const next = [...references, reference];
    const invalid = referenceBatchError(next);
    if (invalid) {
      setReferenceError(invalid);
      return false;
    }
    setReferences(next);
    setVerified([]);
    setReferenceError(null);
    setContextState(null);
    setPreviewRequested(true);
    void verifyReferences(next, workspaceId);
    return true;
  }

  /** 验证引用后把服务端版本回填到发送请求。 */
  async function verifyReferences(
    requested: UserReferenceSpec[],
    workspace: string,
  ): Promise<void> {
    const request = ++referenceGeneration.current;
    setReferenceBusy(true);
    setReferenceError(null);
    try {
      const result = await agentApi.previewReferences(workspace, requested);
      const bound = bindReferenceVersions(requested, result);
      if (!bound) throw new Error("引用校验结果与所选项目不匹配，请重试。");
      const byteError = referenceBatchError(bound);
      if (byteError) throw new Error(byteError);
      if (
        referenceGeneration.current === request &&
        latestWorkspace.current === workspace
      ) {
        setReferences(bound);
        setVerified(result);
      }
    } catch (cause) {
      if (referenceGeneration.current === request)
        setReferenceError(agentError(cause));
    } finally {
      if (referenceGeneration.current === request) setReferenceBusy(false);
    }
  }

  /** 校验并添加指定工作区相对路径与可选行范围。 */
  function addFileReference(): void {
    const filePath = path.trim();
    if (!filePath) {
      setReferenceError("请填写工作区内的相对路径。");
      return;
    }
    const start = startLine.trim() ? Number(startLine) : null;
    const end = endLine.trim() ? Number(endLine) : null;
    if (
      (start !== null && (!Number.isSafeInteger(start) || start < 1)) ||
      (end !== null && (!Number.isSafeInteger(end) || end < 1)) ||
      (start !== null && end !== null && end < start)
    ) {
      setReferenceError("行号必须从 1 开始，结束行不能早于起始行。");
      return;
    }
    const added = addReference({
      kind: "file",
      path: filePath,
      ...(start === null ? {} : { startLine: start }),
      ...(end === null ? {} : { endLine: end }),
    });
    if (added) {
      setPath("");
      setStartLine("");
      setEndLine("");
    }
  }

  /** 添加用户明确粘贴或选中的文本片段。 */
  function addTextReference(): void {
    const value = textSelection.trim();
    if (!value) {
      setReferenceError("请先粘贴或选择要引用的文本。");
      return;
    }
    if (addReference({ kind: "text", text: value })) setTextSelection("");
  }

  /** 添加当前工作区暂存区或未暂存差异。 */
  function addDiffReference(): void {
    addReference({ kind: "gitDiff", staged });
  }

  /** 移除一项引用并重新校验剩余项。 */
  function removeReference(index: number): () => void {
    return () => {
      if (running || referenceBusy) return;
      referenceGeneration.current += 1;
      const next = references.filter((_, current) => current !== index);
      setReferences(next);
      setVerified([]);
      setReferenceError(null);
      setContextState(null);
      setPreviewRequested(true);
      if (next.length) void verifyReferences(next, workspaceId);
    };
  }

  /** 显式请求估算；新聊天仅在此点击后创建空会话。 */
  async function requestContextPreview(): Promise<void> {
    if (
      running ||
      contextBusy ||
      preparingChat ||
      !enabled ||
      !configurationReady
    )
      return;
    setOpen(true);
    setPreviewRequested(true);
    if (effectiveChatId) return;
    if (ensurePending.current) return;
    ensurePending.current = true;
    preserveReferencesOnCreate.current = true;
    setPreparingChat(true);
    setContextError(null);
    try {
      const workspace = workspaceId;
      const session = await ensureChat();
      if (
        latestWorkspace.current !== workspace ||
        session.workspaceId !== workspace
      )
        throw new Error("创建期间工作区已切换，请重新打开上下文预览。");
      setPreviewChatId(session.id);
    } catch (cause) {
      preserveReferencesOnCreate.current = false;
      setContextError(agentError(cause));
    } finally {
      ensurePending.current = false;
      setPreparingChat(false);
    }
  }

  /** 首次发送创建聊天时保留已校验引用，失败重试仍可使用。 */
  function preserveReferencesForChatCreate(): void {
    preserveReferencesOnCreate.current = true;
  }

  /** 聊天创建失败时清除一次性保留标记。 */
  function cancelChatCreatePreservation(): void {
    preserveReferencesOnCreate.current = false;
  }

  /** 处理文件路径输入。 */
  function pathChanged(event: ChangeEvent<HTMLInputElement>): void {
    setPath(event.target.value);
  }

  /** 处理行号输入并在提交时统一验证。 */
  function startLineChanged(event: ChangeEvent<HTMLInputElement>): void {
    setStartLine(event.target.value);
  }

  /** 处理行号输入并在提交时统一验证。 */
  function endLineChanged(event: ChangeEvent<HTMLInputElement>): void {
    setEndLine(event.target.value);
  }

  /** 处理显式文本引用输入。 */
  function textSelectionChanged(event: ChangeEvent<HTMLTextAreaElement>): void {
    setTextSelection(event.target.value);
  }

  /** 在用户确认后选择使用暂存或未暂存 Git 差异。 */
  function stagedChanged(event: ChangeEvent<HTMLInputElement>): void {
    setStaged(event.target.checked);
  }

  /** 发送成功后清理一次性引用和与旧输入绑定的预览。 */
  function clearReferences(): void {
    referenceGeneration.current += 1;
    contextGeneration.current += 1;
    setReferences([]);
    setVerified([]);
    setReferenceError(null);
    setContextState(null);
    setPreviewRequested(false);
  }

  /** 使已显示的上下文预览因记忆条目变化而重新计算。 */
  const refreshPreview = useCallback((): void => {
    setMemoryRevision((current) => current + 1);
  }, []);

  const contextPreview =
    contextState?.key === previewKey ? contextState.result : null;
  const currentVerified = verified.length === references.length;
  return {
    open,
    references,
    verified,
    referenceBusy,
    referenceError,
    contextBusy: contextBusy || preparingChat,
    contextError,
    contextPreview,
    refreshPreview,
    currentVerified,
    canSendReferences: references.length === 0 || currentVerified,
    path,
    startLine,
    endLine,
    textSelection,
    staged,
    show,
    close,
    addFileReference,
    addTextReference,
    addDiffReference,
    removeReference,
    requestContextPreview,
    preserveReferencesForChatCreate,
    cancelChatCreatePreservation,
    pathChanged,
    startLineChanged,
    endLineChanged,
    textSelectionChanged,
    stagedChanged,
    clearReferences,
  };
}
