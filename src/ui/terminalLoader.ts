import { lazy } from "react";

/** 用户创建第一个终端时才加载 xterm，普通 Git 首屏不承担终端包体。 */
export const TerminalPanel = lazy(() =>
  import("../components/TerminalPanel").then((module) => ({
    default: module.TerminalPanel,
  })),
);
