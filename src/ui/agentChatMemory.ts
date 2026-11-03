import type { AgentEvent, ChatRecord } from "../types/agent";

/** 保存当前读取窗口的通知与各聊天草稿；所有数据仅在内存存活。 */
export class AgentChatMemory {
  private loading: string | null = null;
  private pending = new Map<number, AgentEvent>();
  private drafts = new Map<string, string>();

  /** 只为最新一次选中聊天的读取保留通知，防止旧聊天缓存持续增长。 */
  beginLoad(id: string): void {
    if (this.loading !== id) this.pending.clear();
    this.loading = id;
  }

  /** 记录读取期间的持久事件，重复通知只保留同序列的最新值。 */
  receive(event: AgentEvent): void {
    if (event.sessionId === this.loading)
      this.pending.set(event.sequence, event);
  }

  /** 合并磁盘快照、已显示事件和读取窗口通知，不丢失先于 Promise 返回的事件。 */
  merge(view: ChatRecord, current: ChatRecord | null): ChatRecord {
    const events = new Map(view.events.map((event) => [event.sequence, event]));
    if (current?.session.id === view.session.id)
      for (const event of current.events) events.set(event.sequence, event);
    if (this.loading === view.session.id) {
      for (const event of this.pending.values())
        events.set(event.sequence, event);
    }
    return {
      ...view,
      events: [...events.values()].sort((a, b) => a.sequence - b.sequence),
    };
  }

  /** 在 React 状态更新之外结束读取，保证 updater 重放时仍为纯合并。 */
  endLoad(id: string): void {
    if (this.loading === id) {
      this.loading = null;
      this.pending.clear();
    }
  }

  /** 已有聊天以 ID 隔离；新聊天以所在项目隔离。 */
  private key(id: string | null, repositoryPath: string | null): string {
    return id === null ? `project:${repositoryPath ?? ""}` : `chat:${id}`;
  }

  /** 获取指定聊天或项目尚未发送的草稿。 */
  draft(id: string | null, repositoryPath: string | null): string {
    return this.drafts.get(this.key(id, repositoryPath)) ?? "";
  }

  /** 空草稿移除记录，不触及其他聊天的未发送内容。 */
  setDraft(
    id: string | null,
    repositoryPath: string | null,
    text: string,
  ): void {
    const key = this.key(id, repositoryPath);
    if (text) this.drafts.set(key, text);
    else this.drafts.delete(key);
  }
}

/** 与 Rust chars() 对齐；非 BMP 字符计为一个字符，空白名称仍然无效。 */
export function titleValidation(title: string): {
  count: number;
  error: string | null;
} {
  const count = Array.from(title).length;
  return {
    count,
    error: !title.trim() || count > 120 ? "聊天名称须为 1～120 个字符。" : null,
  };
}
