import { invoke, isTauri } from "@tauri-apps/api/core";

export interface TerminalSession {
  sessionId: string;
  repositoryId: string | null;
  displayCwd: string;
  shellLabel: string;
}
export interface TerminalChunk {
  sequence: number;
  bytes: number[];
  finished: boolean;
  exitCode: number | null;
}

/** 终端错误只显示固定说明，不记录或回显未知正文。 */
export function terminalError(error: unknown): string {
  const code =
    typeof error === "object" && error !== null && "code" in error
      ? error.code
      : null;
  switch (code) {
    case "TERMINAL_LIMIT":
      return "最多保留 8 个终端，请先关闭一个终端。";
    case "TERMINAL_SHELL":
      return "Shell 程序不可用，请检查终端设置。";
    case "TERMINAL_DIRECTORY":
      return "终端起始目录不存在或无法访问。";
    case "TERMINAL_INPUT_BUSY":
      return "终端输入队列繁忙，部分输入未发送，请检查终端后重试。";
    case "TERMINAL_UNAVAILABLE":
      return "终端会话已关闭或不可用。";
    default:
      return "终端操作未完成，请检查会话状态后重试。";
  }
}

/** 普通浏览器不能启动本机 shell。 */
async function call<T>(
  command: string,
  args: Record<string, unknown>,
): Promise<T> {
  if (!isTauri()) throw { code: "DESKTOP_REQUIRED" };
  return invoke<T>(command, args);
}
/** 用户明确新建时只提交已保存配置标识。 */
export function createTerminal(
  repositoryId: string | null,
  profileId: string,
): Promise<TerminalSession> {
  return call("create_terminal", {
    repositoryId,
    profileId,
    cols: 80,
    rows: 24,
  });
}
/** 确认已渲染序号后读取下一块，允许重取响应丢失的块。 */
export function readTerminal(
  sessionId: string,
  acknowledgedSequence: number,
): Promise<TerminalChunk> {
  return call("read_terminal", { sessionId, acknowledgedSequence });
}
/** 原始 UTF-8 输入逐块发送，不拼接 shell 命令。 */
export function writeTerminal(
  sessionId: string,
  bytes: Uint8Array,
): Promise<void> {
  return call("write_terminal", { sessionId, bytes: Array.from(bytes) });
}
/** 尺寸仅来自终端视图实际行列数。 */
export function resizeTerminal(
  sessionId: string,
  cols: number,
  rows: number,
): Promise<void> {
  return call("resize_terminal", { sessionId, cols, rows });
}
/** 关闭之前由界面明确确认，会话不会因折叠而结束。 */
export function closeTerminal(sessionId: string): Promise<void> {
  return call("close_terminal", { sessionId });
}
