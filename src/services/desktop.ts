import { invoke, isTauri } from "@tauri-apps/api/core";
import type { AppInfo, AppInfoState } from "../types/app";

/** 只在 Tauri 中读取真实应用信息，浏览器预览不伪造原生响应。 */
export async function readAppInfo(): Promise<AppInfoState> {
  if (!isTauri()) {
    return { status: "preview" };
  }

  try {
    const info = await invoke<AppInfo>("get_app_info");
    return { status: "ready", info };
  } catch {
    return { status: "error" };
  }
}
