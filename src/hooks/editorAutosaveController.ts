export type EditorSaveMode = "manual" | "auto";

export interface EditorAutosaveDocument {
  path: string;
  draft: string;
  baseline: string;
}
export interface EditorAutosaveSnapshot {
  documents: readonly EditorAutosaveDocument[];
}
export interface EditorAutosaveControllerOptions {
  getSnapshot: () => EditorAutosaveSnapshot;
  save: (path: string) => Promise<boolean>;
  blocked: () => boolean;
  mode?: EditorSaveMode;
  delayMs?: number;
}
interface SaveState {
  savingPaths: readonly string[];
  failedPaths: readonly string[];
  composing: boolean;
}

/** 每个仓库独立的保存调度器；所有文档共用一条串行写入队列。 */
export class EditorAutosaveController {
  private options: EditorAutosaveControllerOptions;
  private mode: EditorSaveMode;
  private composing = false;
  private disposed = false;
  private readonly delayMs: number;
  private readonly timers = new Map<string, ReturnType<typeof setTimeout>>();
  private readonly pending = new Map<string, boolean>();
  private readonly failed = new Set<string>();
  private readonly listeners = new Set<() => void>();
  private running: Promise<void> | null = null;
  private savingPath: string | null = null;
  private state: SaveState = {
    savingPaths: [],
    failedPaths: [],
    composing: false,
  };

  constructor(options: EditorAutosaveControllerOptions) {
    this.options = options;
    this.mode = options.mode ?? "manual";
    this.delayMs = options.delayMs ?? 800;
  }
  /** 回调始终读取当前仓库状态，不捕获旧的 React 输入。 */
  updateOptions(
    options: Pick<
      EditorAutosaveControllerOptions,
      "getSnapshot" | "save" | "blocked"
    >,
  ): void {
    this.options = { ...this.options, ...options };
  }
  /** 返回稳定快照，只有状态变化时替换对象。 */
  getSnapshot = (): SaveState => this.state;
  /** 订阅保存、失败与组合输入状态。 */
  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };
  /** 已卸载的任务不再回写界面。 */
  private emit(): void {
    if (this.disposed) return;
    this.state = {
      savingPaths: this.savingPath ? [this.savingPath] : [],
      failedPaths: [...this.failed],
      composing: this.composing,
    };
    for (const listener of this.listeners) listener();
  }
  /** 切为手动只取消尚未开始的自动任务，不中断已经进行的写入。 */
  setMode(mode: EditorSaveMode): void {
    if (this.mode === mode) return;
    this.mode = mode;
    if (mode === "manual") {
      this.clearTimers();
      for (const [path, force] of this.pending)
        if (!force) this.pending.delete(path);
    } else this.scheduleDirtyDocuments();
  }
  /** 输入法候选阶段不保存中间文本。 */
  compositionStart = (): void => {
    this.composing = true;
    this.clearTimers();
    this.emit();
  };
  /** 组合结束后重新开始防抖。 */
  compositionEnd = (): void => {
    this.composing = false;
    this.scheduleDirtyDocuments();
    this.emit();
  };
  /** 仅实际内容输入重置该文档的防抖计时。 */
  notifyChanged(path: string): void {
    if (this.mode === "auto" && !this.composing && !this.failed.has(path))
      this.schedule(path);
  }
  /** 取消所有尚未启动的定时器。 */
  private clearTimers(): void {
    for (const timer of this.timers.values()) clearTimeout(timer);
    this.timers.clear();
  }
  /** 失败文档等待显式重试，避免持续重复失败。 */
  private scheduleDirtyDocuments(): void {
    for (const document of this.options.getSnapshot().documents)
      if (document.draft !== document.baseline)
        this.notifyChanged(document.path);
  }
  /** 按路径合并连续编辑，避免轮询占用事件循环。 */
  private schedule(path: string): void {
    if (this.disposed || this.mode !== "auto" || this.failed.has(path)) return;
    const old = this.timers.get(path);
    if (old) clearTimeout(old);
    this.timers.set(
      path,
      setTimeout(() => {
        this.timers.delete(path);
        this.enqueue(path, false);
      }, this.delayMs),
    );
  }
  /** 手动任务优先保留强制标志，同路径不会重复排队。 */
  private enqueue(path: string, force: boolean): void {
    if (this.disposed || (!force && this.failed.has(path))) return;
    this.pending.set(path, force || this.pending.get(path) === true);
    if (!this.running) {
      // 在异步工作开始前安装等待句柄，flush 直接等待真实在途 Promise。
      this.running = Promise.resolve()
        .then(() => this.drain())
        .finally(() => {
          this.running = null;
        });
    }
  }
  /** 一次只调用一个保存接口，外部忙碌时自动任务延后。 */
  private async drain(): Promise<void> {
    while (!this.disposed && this.pending.size) {
      const [path, force] = this.pending.entries().next().value as [
        string,
        boolean,
      ];
      this.pending.delete(path);
      if (this.composing || (!force && this.mode !== "auto")) continue;
      if (this.options.blocked()) {
        if (!force) this.schedule(path);
        continue;
      }
      const document = this.options
        .getSnapshot()
        .documents.find((item) => item.path === path);
      if (!document || document.draft === document.baseline) continue;
      if (force) this.failed.delete(path);
      this.savingPath = path;
      this.emit();
      let success = false;
      try {
        success = await this.options.save(path);
      } catch {
        success = false;
      }
      if (this.disposed) return;
      this.savingPath = null;
      if (!success) this.failed.add(path);
      else {
        this.failed.delete(path);
        const after = this.options
          .getSnapshot()
          .documents.find((item) => item.path === path);
        if (after && after.draft !== after.baseline) this.notifyChanged(path);
      }
      this.emit();
    }
  }
  /** 快捷键保存也进入串行队列；失败后保留草稿和错误。 */
  async retry(path: string): Promise<boolean> {
    const timer = this.timers.get(path);
    if (timer) clearTimeout(timer);
    this.timers.delete(path);
    this.enqueue(path, true);
    await this.running;
    const document = this.options
      .getSnapshot()
      .documents.find((item) => item.path === path);
    return (
      !this.disposed &&
      !this.failed.has(path) &&
      (!document || document.draft === document.baseline)
    );
  }
  /** 离开前保存当前脏文档并等待真实在途任务，不用忙循环抢占界面。 */
  async flush(): Promise<void> {
    this.clearTimers();
    for (const document of this.options.getSnapshot().documents)
      if (document.draft !== document.baseline)
        this.enqueue(document.path, true);
    await this.running;
  }
  /** React 严格模式重新挂载时恢复调度，旧队列已由清理阶段释放。 */
  activate(): void {
    this.disposed = false;
    this.scheduleDirtyDocuments();
  }
  /** 仓库切换和卸载释放旧会话的定时器；在途结果由文件控制器校验。 */
  dispose(): void {
    this.disposed = true;
    this.clearTimers();
    this.pending.clear();
    this.listeners.clear();
  }
}
