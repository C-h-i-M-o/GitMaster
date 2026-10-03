import { invoke, isTauri } from "@tauri-apps/api/core";
import type { RecentProject } from "../types/recentProjects";

/** 最近项目的桌面端数据契约。 */
export const readRecentProjects = async (): Promise<RecentProject[]> => {
  if (!isTauri()) return [];
  return invoke<RecentProject[]>("read_recent_projects");
};
