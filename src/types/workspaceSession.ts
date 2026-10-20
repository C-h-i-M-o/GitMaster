/** 文档在工作区中的滚动与光标位置。 */
export interface DocumentPosition {
  lineNumber: number;
  column: number;
  scrollTop: number;
  scrollLeft: number;
}

/** 工作区最近一次打开的文档状态。 */
export interface WorkspaceDocument {
  path: string;
  position: DocumentPosition | null;
}

/** 单个项目的工作区会话。 */
export interface WorkspaceSession {
  rootPath: string;
  expandedPaths: string[];
  documents: WorkspaceDocument[];
  activePath: string | null;
  activePage: "files" | "changes" | "history";
}

/** 工作区会话文件的顶层结构。 */
export interface WorkspaceSessionsFile {
  version: 1;
  lastRootPath: string | null;
  projects: WorkspaceSession[];
}
