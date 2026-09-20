import type { Transition } from "motion/react";
import type { AppInfoState } from "../types/app";

export const entranceTransition: Transition = {
  duration: 0.24,
  ease: "easeOut",
};

/** 根据真实连接状态生成提示，不把元数据连接等同于 Git 可用。 */
export function describeRuntime(state: AppInfoState): string {
  switch (state.status) {
    case "loading":
      return "正在读取运行环境…";
    case "preview":
      return "界面预览 · 尚未连接桌面核心";
    case "ready":
      return `${state.info.name} ${state.info.version} · 桌面核心已连接`;
    case "error":
      return "桌面核心连接失败，请检查开发终端后重新启动";
  }
}
