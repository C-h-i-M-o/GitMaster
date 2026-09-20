/** 来自独立 Rust 核心的应用信息；system 是策略，不是安装检测结果。 */
export interface AppInfo {
  name: string;
  version: string;
  gitProvider: "system";
}

/** 将浏览器预览、连接中、已连接和错误明确区分。 */
export type AppInfoState =
  | { status: "loading" }
  | { status: "ready"; info: AppInfo }
  | { status: "preview" }
  | { status: "error" };
