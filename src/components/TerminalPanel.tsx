import type { TerminalPreferences } from "../types/settings";
import type { TerminalSession } from "../services/terminal";
import { useTerminal } from "../hooks/useTerminal";

/** 终端正文由 xterm 呈现，状态与原始创建目录单独显示。 */
export function TerminalPanel({
  session,
  preferences,
}: {
  session: TerminalSession;
  preferences: TerminalPreferences;
}) {
  const terminal = useTerminal(session, preferences);
  return (
    <section className="terminal-panel" aria-label="交互终端">
      <p className="terminal-location" title={session.displayCwd}>
        {session.displayCwd}
      </p>
      {terminal.error && (
        <p role="alert">
          {terminal.error}{" "}
          <button onClick={terminal.retryOutput}>恢复输出读取</button>
        </p>
      )}
      {terminal.exitCode !== null && (
        <p role="status">会话已结束 · 退出码 {terminal.exitCode}</p>
      )}
      <div className="terminal-screen" ref={terminal.container} />
    </section>
  );
}
