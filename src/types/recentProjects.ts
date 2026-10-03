/** 后端持久化的最近项目条目。 */
export interface RecentProject {
  rootPath: string;
  name: string;
  lastOpenedAt: number;
}
