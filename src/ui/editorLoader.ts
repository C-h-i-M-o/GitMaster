import { lazy } from "react";

/** 选择首个可编辑文档后才加载编辑器与本地 Worker。 */
export const FileEditor = lazy(() => import("../components/FileEditor"));
