import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type ChangeEvent,
} from "react";
import { normalizeOperationError } from "../services/gitErrors";
import { readUiPreferences, setUiPreferences } from "../services/git";
import type { OperationError, UiPreferences } from "../types/git";

const DEFAULT_PREFERENCES: UiPreferences = { elasticity: 6, showLabels: true };

export interface PreferencesState {
  saved: UiPreferences | null;
  draft: UiPreferences;
  loading: boolean;
  saving: boolean;
  error: OperationError | null;
  changeElasticity: (event: ChangeEvent<HTMLInputElement>) => void;
  changeLabels: (event: ChangeEvent<HTMLInputElement>) => void;
  save: () => Promise<void>;
  reload: () => void;
  resetDraft: () => void;
}

/** 管理应用偏好的加载、草稿编辑和持久化，并隔离过期异步结果。 */
export function usePreferences(enabled: boolean): PreferencesState {
  const [saved, setSaved] = useState<UiPreferences | null>(null);
  const [draft, setDraft] = useState<UiPreferences>(DEFAULT_PREFERENCES);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<OperationError | null>(null);
  const generation = useRef(0);
  const enabledRef = useRef(enabled);
  const loadingRef = useRef(false);
  const savingRef = useRef(false);
  enabledRef.current = enabled;

  /** 组件卸载时使所有待处理的 IPC 结果失效。 */
  useEffect(
    () => () => {
      generation.current += 1;
      loadingRef.current = false;
      savingRef.current = false;
      enabledRef.current = false;
    },
    [],
  );

  /** 按桌面能力加载真实偏好，失败时保持未加载状态并允许重试。 */
  const reload = useCallback((): void => {
    if (!enabledRef.current || loadingRef.current || savingRef.current) return;
    const token = ++generation.current;
    loadingRef.current = true;
    setLoading(true);
    setError(null);
    void readUiPreferences()
      .then((preferences) => {
        if (generation.current !== token) return;
        setSaved(preferences);
        setDraft(preferences);
        loadingRef.current = false;
        setLoading(false);
      })
      .catch((cause: unknown) => {
        if (generation.current !== token) return;
        loadingRef.current = false;
        setLoading(false);
        setError(normalizeOperationError(cause));
      });
  }, []);

  /** 按桌面能力变化加载设置，禁用时清除残留持久化状态。 */
  useEffect(() => {
    enabledRef.current = enabled;
    if (!enabled) {
      generation.current += 1;
      loadingRef.current = false;
      savingRef.current = false;
      setLoading(false);
      setSaving(false);
      setSaved(null);
      setDraft(DEFAULT_PREFERENCES);
      return;
    }
    reload();
  }, [enabled, reload]);

  /** 将弹性滑块输入限制在后端接受的范围内。 */
  const changeElasticity = useCallback(
    (event: ChangeEvent<HTMLInputElement>): void => {
      if (!enabledRef.current || loadingRef.current || savingRef.current)
        return;
      const value = Math.min(10, Math.max(1, Number(event.target.value)));
      setDraft((current) => ({
        ...current,
        elasticity: Number.isFinite(value) ? value : current.elasticity,
      }));
    },
    [],
  );

  /** 更新标签显示草稿，不触发持久化。 */
  const changeLabels = useCallback(
    (event: ChangeEvent<HTMLInputElement>): void => {
      if (!enabledRef.current || loadingRef.current || savingRef.current)
        return;
      setDraft((current) => ({ ...current, showLabels: event.target.checked }));
    },
    [],
  );

  /** 仅在成功加载后保存当前草稿，失败时保留草稿和已保存值。 */
  const save = useCallback(async (): Promise<void> => {
    if (
      !enabledRef.current ||
      loadingRef.current ||
      savingRef.current ||
      saved === null
    )
      return;
    const token = ++generation.current;
    const next = draft;
    savingRef.current = true;
    setSaving(true);
    setError(null);
    try {
      const persisted = await setUiPreferences(
        next.elasticity,
        next.showLabels,
      );
      if (generation.current !== token) return;
      setSaved(persisted);
      setDraft(persisted);
      savingRef.current = false;
      setSaving(false);
    } catch (cause: unknown) {
      if (generation.current !== token) return;
      savingRef.current = false;
      setSaving(false);
      setError(normalizeOperationError(cause));
    }
  }, [draft, saved]);

  /** 将草稿恢复为最近一次成功加载或保存的值。 */
  const resetDraft = useCallback((): void => {
    if (loadingRef.current || savingRef.current) return;
    setDraft(saved ?? DEFAULT_PREFERENCES);
  }, [saved]);

  return {
    saved,
    draft,
    loading,
    saving,
    error,
    changeElasticity,
    changeLabels,
    save,
    reload,
    resetDraft,
  };
}
