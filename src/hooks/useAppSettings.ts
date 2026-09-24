import { useCallback, useEffect, useRef, useState } from "react";
import {
  applyAppSettings,
  normalizeSettingsError,
  readAppSettings,
} from "../services/settings";
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

/** 管理六类设置的统一草稿；保存失败保留草稿，异步旧响应不覆盖新会话。 */
export function useAppSettings(
  enabled: boolean,
  onApplied: (gitChanged: boolean) => void,
) {
  const [saved, setSaved] = useState<SettingsSnapshot | null>(null);
  const [draft, setDraft] = useState<AppSettings>(defaultSettings);
  const [activity, setActivity] = useState<"idle" | "loading" | "saving">(
    "idle",
  );
  const [error, setError] = useState<SettingsError | null>(null);
  const generation = useRef(0);
  const busy = useRef(false);
  const available = useRef(enabled);
  const applied = useRef(onApplied);
  available.current = enabled;
  applied.current = onApplied;

  /** 重新读取丢弃旧草稿，界面须先处理未保存确认。 */
  const reload = useCallback(async (): Promise<void> => {
    if (!available.current || busy.current) return;
    const token = ++generation.current;
    busy.current = true;
    setActivity("loading");
    setError(null);
    try {
      const snapshot = await readAppSettings();
      if (generation.current !== token) return;
      setSaved(snapshot);
      setDraft(structuredClone(snapshot.settings));
    } catch (cause: unknown) {
      if (generation.current === token) setError(normalizeSettingsError(cause));
    } finally {
      if (generation.current === token) {
        busy.current = false;
        setActivity("idle");
      }
    }
  }, []);

  /** 桌面能力切换和卸载使正在返回的旧请求失效。 */
  useEffect(() => {
    available.current = enabled;
    busy.current = false;
    setActivity("idle");
    setSaved(null);
    setDraft(defaultSettings());
    setError(null);
    if (enabled) void reload();
    return () => {
      ++generation.current;
      available.current = false;
      busy.current = false;
    };
  }, [enabled, reload]);

  /** 所有编辑只更新草稿；保存期间禁止编辑避免误清空新输入。 */
  function edit(update: (current: AppSettings) => AppSettings): void {
    if (!available.current || busy.current || saved === null) return;
    setDraft(update);
    setError(null);
  }

  /** 丢弃草稿时恢复最近成功读取或保存的快照。 */
  function cancel(): void {
    if (busy.current) return;
    setDraft(saved ? structuredClone(saved.settings) : defaultSettings());
    setError(null);
  }

  /** 默认值只作用于当前分类，保留其余未保存编辑。 */
  function restore(category: SettingsCategory): void {
    edit((current) => resetSettingsCategory(current, category));
  }

  /** 整份设置一次提交，过期错误留给用户核对，绝不自动覆盖。 */
  async function save(): Promise<boolean> {
    if (!available.current || busy.current || saved === null) return false;
    if (!settingsDirty(saved.settings, draft)) return true;
    const token = ++generation.current;
    const gitChanged = saved.settings.gitPath !== draft.gitPath;
    busy.current = true;
    setActivity("saving");
    setError(null);
    try {
      const snapshot = await applyAppSettings(
        saved.revision,
        structuredClone(draft),
      );
      if (generation.current !== token) return false;
      setSaved(snapshot);
      setDraft(structuredClone(snapshot.settings));
      applied.current(gitChanged);
      return true;
    } catch (cause: unknown) {
      if (generation.current === token) setError(normalizeSettingsError(cause));
      return false;
    } finally {
      if (generation.current === token) {
        busy.current = false;
        setActivity("idle");
      }
    }
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
    dirty: saved !== null && settingsDirty(saved.settings, draft),
    editable: enabled && activity === "idle" && saved !== null,
  };
}
