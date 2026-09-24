import { invoke, isTauri } from "@tauri-apps/api/core";
import type {
  AppSettings,
  SettingsError,
  SettingsFieldError,
  SettingsSnapshot,
} from "../types/settings";

/** 保留设置专用字段错误，不把未知异常或命令参数展示到界面。 */
export function normalizeSettingsError(error: unknown): SettingsError {
  if (
    typeof error !== "object" ||
    error === null ||
    !("code" in error) ||
    typeof error.code !== "string"
  )
    return { code: "SETTINGS_IO", fieldErrors: [] };
  const fieldErrors: SettingsFieldError[] = [];
  if ("fieldErrors" in error && Array.isArray(error.fieldErrors)) {
    for (const item of error.fieldErrors) {
      if (
        typeof item === "object" &&
        item !== null &&
        "field" in item &&
        "message" in item &&
        typeof item.field === "string" &&
        typeof item.message === "string"
      )
        fieldErrors.push({ field: item.field, message: item.message });
    }
  }
  return { code: error.code, fieldErrors };
}

/** 普通浏览器不伪造成功的设置持久化。 */
async function settingsCall<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  if (!isTauri())
    throw { code: "DESKTOP_REQUIRED", fieldErrors: [] } satisfies SettingsError;
  try {
    return await invoke<T>(command, args);
  } catch (error: unknown) {
    throw normalizeSettingsError(error);
  }
}

/** 读取统一设置及其并发修改标识。 */
export function readAppSettings(): Promise<SettingsSnapshot> {
  return settingsCall("read_app_settings");
}

/** 全量提交草稿；后端拒绝过期 revision，绝不自动重试覆盖新值。 */
export function applyAppSettings(
  expectedRevision: string,
  draft: AppSettings,
): Promise<SettingsSnapshot> {
  return settingsCall("apply_app_settings", { expectedRevision, draft });
}
