import { lazy } from "react";
/** AI 消息渲染依赖仅在进入 AI 工作区时加载。 */
export const AiWorkspace = lazy(() => import("../components/ai/AiWorkspace"));
