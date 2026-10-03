import type {
  AppSettings,
  SettingsError,
  SettingsPatch,
  SettingsSnapshot,
} from "../types/settings.ts";
import {
  changedSettingsPatches,
  mergeSettingsPatch,
  settingsPatchKey,
  settingsPatchField,
} from "../ui/settingsPatch.ts";

type SavePatch = (
  revision: string,
  patch: SettingsPatch,
) => Promise<SettingsSnapshot>;
export interface SettingsAutosaveState {
  saved: SettingsSnapshot;
  draft: AppSettings;
  saving: boolean;
  error: SettingsError | null;
}

/** 按字段串行保存，保留在途新输入和失败字段，不把其他字段的成功当成全部已保存。 */
export class SettingsAutosaveController {
  private state: SettingsAutosaveState;
  private readonly savePatch: SavePatch;
  private readonly readLatest?: () => Promise<SettingsSnapshot>;
  private readonly changed: (
    state: SettingsAutosaveState,
    gitChanged: boolean,
  ) => void;
  private readonly pending = new Map<string, SettingsPatch>();
  private readonly failed = new Map<
    string,
    { patch: SettingsPatch; error: SettingsError }
  >();
  private timer: ReturnType<typeof setTimeout> | null = null;
  private inFlight: Promise<void> | null = null;
  private disposed = false;
  private composing = false;

  /** 从后端已核实快照启动本轮队列。 */
  constructor(
    snapshot: SettingsSnapshot,
    savePatch: SavePatch,
    changed: (
      state: SettingsAutosaveState,
      gitChanged: boolean,
    ) => void = () => {},
    readLatest?: () => Promise<SettingsSnapshot>,
  ) {
    this.state = {
      saved: snapshot,
      draft: structuredClone(snapshot.settings),
      saving: false,
      error: null,
    };
    this.savePatch = savePatch;
    this.readLatest = readLatest;
    this.changed = changed;
  }
  /** 返回最新队列视图，不暴露可变映射。 */
  getSnapshot(): SettingsAutosaveState {
    return this.state;
  }
  /** 统一发布保存状态和字段错误。 */
  private publish(gitChanged = false): void {
    const failures = [...this.failed.values()];
    this.state = {
      ...this.state,
      error: failures[0]
        ? {
            code: failures[0].error.code,
            fieldErrors: failures.flatMap((item) => item.error.fieldErrors),
          }
        : null,
    };
    if (!this.disposed) this.changed(this.state, gitChanged);
  }
  /** 编辑产生精确字段补丁，避免同分类的无效值连带阻止其他输入。 */
  edit(update: (current: AppSettings) => AppSettings, immediate = false): void {
    if (this.disposed) return;
    const next = update(this.state.draft);
    const patches = changedSettingsPatches(this.state.draft, next);
    this.state = { ...this.state, draft: next };
    for (const patch of patches) this.enqueue(patch, immediate);
    this.publish();
  }
  /** 新输入替换同字段旧请求；防抖从最后一次输入重新计时。 */
  enqueue(patch: SettingsPatch, immediate = false): void {
    if (this.disposed) return;
    const key = settingsPatchKey(patch);
    this.pending.set(key, patch);
    this.failed.delete(key);
    if (this.timer) clearTimeout(this.timer);
    this.timer = null;
    if (this.composing) return;
    if (immediate) void this.drain();
    else
      this.timer = setTimeout(() => {
        this.timer = null;
        void this.drain();
      }, 400);
  }
  /** 输入法尚在组合时不保存未提交的文字。 */
  setComposing(value: boolean): void {
    this.composing = value;
    if (this.timer) clearTimeout(this.timer);
    this.timer = null;
    if (!value && this.pending.size)
      this.timer = setTimeout(() => {
        this.timer = null;
        void this.drain();
      }, 400);
  }
  /** 单次失败只阻止对应字段，后续有效字段继续保存。 */
  private async drain(): Promise<void> {
    if (this.disposed || this.composing) return;
    if (this.inFlight) {
      await this.inFlight;
      return this.drain();
    }
    if (!this.pending.size) return;
    this.inFlight = (async () => {
      this.state = { ...this.state, saving: true };
      this.publish();
      while (this.pending.size && !this.disposed && !this.composing) {
        const entry = this.pending.entries().next().value;
        if (!entry) break;
        const [key, patch] = entry;
        this.pending.delete(key);
        try {
          let before = this.state.saved;
          let snapshot: SettingsSnapshot;
          try {
            snapshot = await this.savePatch(before.revision, patch);
          } catch (cause: unknown) {
            if (
              !this.readLatest ||
              typeof cause !== "object" ||
              cause === null ||
              !("code" in cause) ||
              cause.code !== "STALE_SETTINGS"
            )
              throw cause;
            const latest = await this.readLatest();
            if (this.disposed) return;
            const conflict = changedSettingsPatches(
              before.settings,
              latest.settings,
            ).some((change) => settingsPatchKey(change) === key);
            let draft = latest.settings;
            for (const edit of changedSettingsPatches(
              before.settings,
              this.state.draft,
            ))
              draft = mergeSettingsPatch(draft, edit);
            this.state = { ...this.state, saved: latest, draft };
            if (conflict)
              throw {
                code: "STALE_SETTINGS",
                fieldErrors: [
                  {
                    field: settingsPatchField(patch),
                    message:
                      "此项已在其他位置修改，当前输入已保留。请核对后重试保存。",
                  },
                ],
              };
            before = latest;
            snapshot = await this.savePatch(latest.revision, patch);
          }
          if (this.disposed) return;
          // 只把没有本地编辑的字段跟随后端更新，新输入始终保留。
          const edits = changedSettingsPatches(
            before.settings,
            this.state.draft,
          );
          let draft = snapshot.settings;
          for (const edit of edits) draft = mergeSettingsPatch(draft, edit);
          this.state = { ...this.state, saved: snapshot, draft };
          this.failed.delete(key);
          this.publish(before.settings.gitPath !== snapshot.settings.gitPath);
        } catch (cause: unknown) {
          const error: SettingsError =
            typeof cause === "object" &&
            cause !== null &&
            "code" in cause &&
            typeof cause.code === "string"
              ? {
                  code: cause.code,
                  fieldErrors:
                    "fieldErrors" in cause && Array.isArray(cause.fieldErrors)
                      ? cause.fieldErrors
                      : [],
                }
              : { code: "SETTINGS_IO", fieldErrors: [] };
          if (!this.pending.has(key)) this.failed.set(key, { patch, error });
          this.publish();
        }
      }
      this.state = { ...this.state, saving: false };
      this.publish();
    })();
    try {
      await this.inFlight;
    } finally {
      this.inFlight = null;
    }
  }
  /** 明确保存/离开时立即清空队列，可重试上次失败但不会无限自动重放。 */
  async flush(retry = true): Promise<boolean> {
    if (this.timer) clearTimeout(this.timer);
    this.timer = null;
    if (retry)
      for (const [key, failure] of this.failed)
        if (!this.pending.has(key)) this.pending.set(key, failure.patch);
    await this.drain();
    return (
      !this.composing &&
      !this.failed.size &&
      !this.pending.size &&
      !this.disposed
    );
  }
  /** 用户明确放弃时只清理尚未持久化的输入；在途请求需先由界面等待。 */
  reset(): void {
    if (this.inFlight) return;
    if (this.timer) clearTimeout(this.timer);
    this.timer = null;
    this.pending.clear();
    this.failed.clear();
    this.state = {
      ...this.state,
      draft: structuredClone(this.state.saved.settings),
      error: null,
    };
    this.publish();
  }
  /** 卸载后取消未来保存和通知，在途响应不写回新会话。 */
  dispose(): void {
    this.disposed = true;
    if (this.timer) clearTimeout(this.timer);
    this.timer = null;
    this.pending.clear();
  }
}
