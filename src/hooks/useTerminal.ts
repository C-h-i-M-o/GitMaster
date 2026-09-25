import { useEffect, useRef, useState } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import type { TerminalPreferences } from "../types/settings";
import {
  readTerminal,
  resizeTerminal,
  terminalError,
  writeTerminal,
  type TerminalSession,
} from "../services/terminal";
import "@xterm/xterm/css/xterm.css";

/** 一个标签拥有一个 xterm；折叠继续消费有界输出，不重建 shell。 */
export function useTerminal(
  session: TerminalSession,
  preferences: TerminalPreferences,
) {
  const container = useRef<HTMLDivElement>(null);
  const terminal = useRef<Terminal | null>(null);
  const initial = useRef(preferences);
  const [error, setError] = useState<string | null>(null);
  const [exitCode, setExitCode] = useState<number | null>(null);
  const retry = useRef<() => void>(() => {});
  const resizeView = useRef<() => void>(() => {});
  useEffect(() => {
    if (!container.current) return;
    const prefs = initial.current;
    const view = new Terminal({
      fontFamily: prefs.fontFamily,
      fontSize: prefs.fontSize,
      cursorStyle: prefs.cursorStyle,
      cursorBlink: prefs.cursorBlink,
      scrollback: prefs.scrollbackLines,
      theme: { background: "#151820", foreground: "#e5e7eb" },
      allowProposedApi: false,
    });
    const fit = new FitAddon();
    view.loadAddon(fit);
    view.open(container.current);
    terminal.current = view;
    // 仓库程序输出不能写系统剪贴板；不安装自动打开链接的插件。
    const clipboard = view.parser.registerOscHandler(52, () => true);
    let active = true;
    let reading = false;
    let ack = 0;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let ended = false;
    let writing = false;
    const input: Uint8Array[] = [];
    let inputBytes = 0;
    /** xterm 消费完成后才确认输出，失败不跳过未确认序号。 */
    async function poll(): Promise<void> {
      if (!active || reading || ended) return;
      reading = true;
      try {
        const chunk = await readTerminal(session.sessionId, ack);
        if (!active) return;
        if (chunk.bytes.length) {
          await new Promise<void>((resolve) =>
            view.write(new Uint8Array(chunk.bytes), resolve),
          );
          if (!active) return;
          ack = chunk.sequence;
        }
        if (chunk.finished) {
          ended = true;
          view.options.disableStdin = true;
          setExitCode(chunk.exitCode);
        } else
          timer = setTimeout(() => void poll(), chunk.bytes.length ? 0 : 40);
      } catch (cause: unknown) {
        if (active) setError(terminalError(cause));
      } finally {
        reading = false;
      }
    }
    retry.current = () => {
      clearTimeout(timer);
      setError(null);
      void poll();
    };
    /** 输入逐次等待 IPC，不无限积累粘贴内容，也不自动重放失败输入。 */
    async function flushInput(): Promise<void> {
      if (writing) return;
      writing = true;
      try {
        while (active && input.length) {
          const bytes = input.shift();
          if (!bytes) break;
          inputBytes -= bytes.length;
          await writeTerminal(session.sessionId, bytes);
        }
      } catch (cause: unknown) {
        input.length = 0;
        inputBytes = 0;
        if (active) setError(terminalError(cause));
      } finally {
        writing = false;
      }
    }
    const typing = view.onData((data) => {
      if (ended) return;
      const bytes = new TextEncoder().encode(data);
      if (inputBytes + bytes.length > 65536) {
        setError("输入过长，未发送本次内容。请分段粘贴（每次不超过 64 KiB）。");
        return;
      }
      for (let start = 0; start < bytes.length; start += 4096)
        input.push(bytes.slice(start, start + 4096));
      inputBytes += bytes.length;
      void flushInput();
    });
    /** 隐藏标签不发送零尺寸，重新显示时按实际空间更新。 */
    function resize(): void {
      if (
        !active ||
        !container.current?.clientWidth ||
        !container.current.clientHeight
      )
        return;
      fit.fit();
      if (view.cols > 500 || view.rows > 300) return;
      void resizeTerminal(session.sessionId, view.cols, view.rows).catch(
        (cause: unknown) => {
          if (active) setError(terminalError(cause));
        },
      );
    }
    const observer = new ResizeObserver(resize);
    resizeView.current = resize;
    observer.observe(container.current);
    resize();
    void poll();
    return () => {
      active = false;
      clearTimeout(timer);
      retry.current = () => {};
      resizeView.current = () => {};
      observer.disconnect();
      typing.dispose();
      clipboard.dispose();
      view.dispose();
      terminal.current = null;
    };
  }, [session.sessionId]);
  useEffect(() => {
    if (terminal.current)
      Object.assign(terminal.current.options, {
        fontFamily: preferences.fontFamily,
        fontSize: preferences.fontSize,
        cursorStyle: preferences.cursorStyle,
        cursorBlink: preferences.cursorBlink,
      });
    resizeView.current();
  }, [
    preferences.fontFamily,
    preferences.fontSize,
    preferences.cursorStyle,
    preferences.cursorBlink,
  ]);
  /** 只恢复输出读取，未知输入不重发。 */
  function retryOutput(): void {
    retry.current();
  }
  return { container, error, exitCode, retryOutput };
}
