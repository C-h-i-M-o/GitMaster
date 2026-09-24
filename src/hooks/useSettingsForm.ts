import { useEffect, useRef, useState, type ChangeEvent } from "react";
import type { useAppSettings } from "./useAppSettings";
import type {
  AppSettings,
  TerminalProfile,
  SettingsCategory,
} from "../types/settings";
import { settingsFieldLocation } from "../ui/settingsField";
import { removeTerminalProfile } from "../ui/settingsDraft";
import { chooseGitPath } from "../services/git";

type SettingsState = ReturnType<typeof useAppSettings>;
type Input = ChangeEvent<HTMLInputElement | HTMLSelectElement>;

/** 为设置展示层提供受控事件；所有值先进入草稿，不直接持久化。 */
export function useSettingsForm(
  settings: SettingsState,
  category: SettingsCategory,
  selectCategory: (category: SettingsCategory) => () => void,
) {
  const form = useRef<HTMLDivElement>(null);
  const [focusField, setFocusField] = useState<string | null>(null);
  useEffect(() => {
    if (!focusField || settingsFieldLocation(focusField)?.category !== category)
      return;
    const elements = form.current?.querySelectorAll<HTMLElement>(
      "[data-settings-field]",
    );
    const element = Array.from(elements ?? []).find(
      (item) => item.dataset.settingsField === focusField,
    );
    if (element) {
      element.scrollIntoView({ block: "nearest" });
      element.focus();
    }
    setFocusField(null);
  }, [category, focusField]);
  /** 错误定位只切换当前设置分类，不保存或丢弃其他草稿。 */
  function locateError(field: string): () => void {
    return () => {
      const location = settingsFieldLocation(field);
      if (!location) return;
      selectCategory(location.category)();
      setFocusField(field);
    };
  }
  const [pickerError, setPickerError] = useState("");
  const mounted = useRef(true);
  const picking = useRef(false);
  /** 离开设置页后不接受晚到的文件选择器结果。 */
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  /** 把字段更新封装为事件回调，展示组件不承担业务逻辑。 */
  function field(update: (draft: AppSettings, value: string) => AppSettings) {
    return (event: Input): void => {
      const value = event.target.value;
      settings.edit((draft) => update(draft, value));
    };
  }
  /** 通用显示选项只接受输入框提供的有限数字。 */
  function displayField(
    section: "terminal" | "editor",
    name: "fontFamily" | "fontSize" | "scrollbackLines",
  ) {
    return field((draft, value) => {
      if (name === "scrollbackLines" && section !== "terminal") return draft;
      const parsed = name === "fontFamily" ? value : Number(value);
      if (typeof parsed === "number" && !Number.isFinite(parsed)) return draft;
      return { ...draft, [section]: { ...draft[section], [name]: parsed } };
    });
  }
  /** 编辑单个配置时保留其稳定标识和其他配置。 */
  function profileField(
    id: string,
    name: "name" | "executablePath" | "directory" | "path",
  ) {
    return field((draft, value) => ({
      ...draft,
      terminal: {
        ...draft.terminal,
        profiles: draft.terminal.profiles.map((profile) => {
          if (profile.profileId !== id) return profile;
          if (name === "directory")
            return {
              ...profile,
              cwd:
                value === "home"
                  ? { kind: "home" }
                  : value === "fixed"
                    ? { kind: "fixed", path: "" }
                    : { kind: "project" },
            };
          if (name === "path")
            return { ...profile, cwd: { kind: "fixed", path: value } };
          return {
            ...profile,
            [name]: name === "executablePath" && value === "" ? null : value,
          };
        }),
      },
    }));
  }
  /** 新增独立配置，不启动 shell。 */
  function addProfile(): void {
    settings.edit((draft) =>
      draft.terminal.profiles.length >= 32
        ? draft
        : {
            ...draft,
            terminal: {
              ...draft.terminal,
              profiles: [
                ...draft.terminal.profiles,
                {
                  profileId: crypto.randomUUID(),
                  name: "新终端",
                  executablePath: null,
                  args: [],
                  cwd: { kind: "project" },
                },
              ],
            },
          },
    );
  }
  /** 删除配置仅影响后续新终端。 */
  function removeProfile(id: string): () => void {
    return () => settings.edit((draft) => removeTerminalProfile(draft, id));
  }
  /** 参数逐项编辑，空格和引号保留为原始参数内容。 */
  function argument(id: string, index: number) {
    return field((draft, value) =>
      updateProfile(draft, id, (profile) => ({
        ...profile,
        args: profile.args.map((arg, i) => (i === index ? value : arg)),
      })),
    );
  }
  /** 新增参数不进行 shell 字符串拆分。 */
  function addArgument(id: string): () => void {
    return () =>
      settings.edit((draft) =>
        updateProfile(draft, id, (profile) =>
          profile.args.length >= 64
            ? profile
            : { ...profile, args: [...profile.args, ""] },
        ),
      );
  }
  /** 删除明确选定的启动参数。 */
  function removeArgument(id: string, index: number): () => void {
    return () =>
      settings.edit((draft) =>
        updateProfile(draft, id, (profile) => ({
          ...profile,
          args: profile.args.filter((_, i) => i !== index),
        })),
      );
  }
  /** Git 选择器只更新候选路径，点击应用后才验证和生效。 */
  async function chooseGit(): Promise<void> {
    if (picking.current || !settings.editable) return;
    picking.current = true;
    setPickerError("");
    try {
      const path = await chooseGitPath();
      if (mounted.current && path !== null)
        settings.edit((draft) => ({ ...draft, gitPath: path }));
    } catch {
      if (mounted.current) setPickerError("无法打开 Git 选择器，请重试。");
    } finally {
      picking.current = false;
    }
  }
  return {
    form,
    locateError,
    pickerError,
    chooseGit,
    displayField,
    profileField,
    addProfile,
    removeProfile,
    argument,
    addArgument,
    removeArgument,
    gitPath: field((draft, value) => ({ ...draft, gitPath: value || null })),
    elasticity: field((draft, value) => ({
      ...draft,
      uiPreferences: { ...draft.uiPreferences, elasticity: Number(value) },
    })),
    labels: (event: ChangeEvent<HTMLInputElement>): void =>
      settings.edit((draft) => ({
        ...draft,
        uiPreferences: {
          ...draft.uiPreferences,
          showLabels: event.target.checked,
        },
      })),
    logLevel: field((draft, value) =>
      ["auto", "error", "warn", "info", "debug", "trace"].includes(value)
        ? {
            ...draft,
            logLevel:
              value === "auto"
                ? null
                : (value as NonNullable<AppSettings["logLevel"]>),
          }
        : draft,
    ),
    defaultProfile: field((draft, value) => ({
      ...draft,
      terminal: { ...draft.terminal, defaultProfileId: value },
    })),
    cursorStyle: field((draft, value) =>
      value === "block" || value === "bar" || value === "underline"
        ? { ...draft, terminal: { ...draft.terminal, cursorStyle: value } }
        : draft,
    ),
    cursorBlink: (event: ChangeEvent<HTMLInputElement>): void =>
      settings.edit((draft) => ({
        ...draft,
        terminal: { ...draft.terminal, cursorBlink: event.target.checked },
      })),
    tabSize: field((draft, value) => {
      const size = Number(value);
      return size === 2 || size === 4 || size === 8
        ? { ...draft, editor: { ...draft.editor, tabSize: size } }
        : draft;
    }),
    wordWrap: field((draft, value) =>
      value === "on" || value === "off"
        ? { ...draft, editor: { ...draft.editor, wordWrap: value } }
        : draft,
    ),
    externalApp: field((draft, value) =>
      value === "fileManager" || value === "vsCode" || value === "terminal"
        ? { ...draft, externalOpen: { defaultAppId: value } }
        : draft,
    ),
  };
}

/** 参数更新复用不可变配置映射，避免修改已保存快照。 */
function updateProfile(
  draft: AppSettings,
  id: string,
  update: (profile: TerminalProfile) => TerminalProfile,
): AppSettings {
  return {
    ...draft,
    terminal: {
      ...draft.terminal,
      profiles: draft.terminal.profiles.map((profile) =>
        profile.profileId === id ? update(profile) : profile,
      ),
    },
  };
}
