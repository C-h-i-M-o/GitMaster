import { useCallback, useEffect, useRef, useState } from "react";
import {
  applySettingsPatch,
  normalizeSettingsError,
  readAppSettings,
} from "../services/settings";
import { SettingsAutosaveController } from "../services/settingsAutosaveController";
import {
  defaultSettings,
  resetSettingsCategory,
  settingsDirty,
} from "../ui/settingsDraft";
import type {
  AppSettings,
  SettingsCategory,
  SettingsError,
  SettingsSnapshot,
} from "../types/settings";

/** 自动持久化设置，保留失败输入，并隔离不同加载代次的异步响应。 */
export function useAppSettings(
  enabled: boolean,
  onApplied: (gitChanged: boolean) => void,
  canChangeGit: () => boolean = () => true,
) {
  const [saved, setSaved] = useState<SettingsSnapshot | null>(null);
  const [draft, setDraft] = useState<AppSettings>(defaultSettings);
  const [activity, setActivity] = useState<"idle" | "loading" | "saving">(
    "idle",
  );
  const [error, setError] = useState<SettingsError | null>(null);
  const controller = useRef<SettingsAutosaveController | null>(null);
  const generation = useRef(0);
  const current = useRef({ enabled, onApplied, canChangeGit });
  current.current = { enabled, onApplied, canChangeGit };
  /** 加载新快照时先取消旧队列，禁止迟到结果覆盖当前表单。 */
  const reload = useCallback(async (): Promise<void> => {
    if (!current.current.enabled) return;
    const token = ++generation.current;
    controller.current?.dispose();
    controller.current = null;
    setActivity("loading");
    setError(null);
    try {
      const snapshot = await readAppSettings();
      if (token !== generation.current) return;
      setSaved(snapshot);
      setDraft(structuredClone(snapshot.settings));
      let appliedRevision = snapshot.revision;
      controller.current = new SettingsAutosaveController(
        snapshot,
        async (revision, patch) => {
          if (patch.kind === "git" && !current.current.canChangeGit())
            throw {
              code: "INVALID_INPUT",
              fieldErrors: [
                {
                  field: "gitPath",
                  message: "请先完成当前操作，并保存或放弃文件与冲突草稿。",
                },
              ],
            };
          return applySettingsPatch(revision, patch);
        },
        (state, gitChanged) => {
          if (token !== generation.current) return;
          setSaved(state.saved);
          setDraft(state.draft);
          setError(state.error);
          setActivity(state.saving ? "saving" : "idle");
          if (state.saved.revision !== appliedRevision) {
            appliedRevision = state.saved.revision;
            current.current.onApplied(gitChanged);
          }
        },
        readAppSettings,
      );
    } catch (cause: unknown) {
      if (token === generation.current) setError(normalizeSettingsError(cause));
    } finally {
      if (token === generation.current) setActivity("idle");
    }
  }, []);
  useEffect(() => {
    setSaved(null);
    setDraft(defaultSettings());
    if (enabled) void reload();
    return () => {
      ++generation.current;
      controller.current?.dispose();
      controller.current = null;
    };
  }, [enabled, reload]);
  /** 控件可选择立即保存；文本默认按停止输入防抖。 */
  function edit(
    update: (settings: AppSettings) => AppSettings,
    immediate = false,
  ): void {
    controller.current?.edit(update, immediate);
  }
  /** 离开或手动重试等待当前及后续请求全部完成。 */
  async function save(): Promise<boolean> {
    return controller.current ? controller.current.flush() : false;
  }
  /** 明确放弃无效或失败输入时恢复实际已保存快照。 */
  function cancel(): void {
    controller.current?.reset();
  }
  /** 保留内部兼容接口，界面不再展示整类恢复按钮。 */
  function restore(category: SettingsCategory): void {
    edit((value) => resetSettingsCategory(value, category), true);
  }
  /** 整个表单共享输入法组合门禁。 */
  function compositionStart(): void {
    controller.current?.setComposing(true);
  }
  /** 组合完成后重新启动防抖。 */
  function compositionEnd(): void {
    controller.current?.setComposing(false);
  }
  /** 失焦时刷新队列，失败由字段提示承载。 */
  function flushOnBlur(): void {
    void controller.current?.flush(false);
  }
  return {
    saved,
    draft,
    activity,
    error,
    edit,
    reload,
    cancel,
    restore,
    save,
    compositionStart,
    compositionEnd,
    flushOnBlur,
    dirty: saved !== null && settingsDirty(saved.settings, draft),
    editable: enabled && activity !== "loading" && saved !== null,
  };
}
