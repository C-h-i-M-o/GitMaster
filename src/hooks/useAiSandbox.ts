import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { aiError } from "../services/ai";

interface Status {
  ready: boolean;
  root: string;
  operation: string;
  win32Code: number | null;
  writeDac: boolean;
  writeOwner: boolean;
  shellAvailable: boolean;
}
interface Repair {
  confirmationId: string;
  root: string;
  backupPath: string;
  detail: string;
}
/** ACL 操作仅由用户在设置中发起；预览票据绝不交给模型。 */
export function useAiSandbox(repositoryId: string | null) {
  const generation = useRef(0);
  const [status, setStatus] = useState<Status | null>(null);
  const [repair, setRepair] = useState<Repair | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [result, setResult] = useState("");
  const [action, setAction] = useState<"check" | "prepare" | "execute" | null>(
    null,
  );
  const inFlight = useRef(false);
  useEffect(() => {
    generation.current++;
    setBusy(false);
    inFlight.current = false;
    setAction(null);
    setStatus(null);
    setRepair(null);
    setError("");
    setResult("");
    return () => {
      generation.current++;
    };
  }, [repositoryId]);
  /** 串行执行用户操作，失败保留具体诊断和备份信息。 */
  async function run(action: "check" | "prepare" | "execute"): Promise<void> {
    if (!repositoryId || inFlight.current) return;
    inFlight.current = true;
    const token = ++generation.current;
    setBusy(true);
    setAction(action);
    setError("");
    try {
      if (action === "check") {
        setResult("");
        setRepair(null);
        const response = await invoke<Status>("read_ai_sandbox_status", {
          repositoryId,
        });
        if (token === generation.current) setStatus(response);
      } else if (action === "prepare") {
        const response = await invoke<Repair>("prepare_ai_sandbox_repair", {
          repositoryId,
        });
        if (token === generation.current) setRepair(response);
      } else if (repair) {
        const response = await invoke<{
          ok: boolean;
          status: Status;
          backupPath: string;
          detail: string;
          error: string;
        }>("execute_ai_sandbox_repair", {
          repositoryId,
          confirmationId: repair.confirmationId,
        });
        if (token !== generation.current) return;
        setStatus(response.status);
        setRepair(null);
        setResult(
          `${response.ok ? "修复完成" : "修复未完成"}。备份：${response.backupPath}\n${response.detail}\n${response.error}`,
        );
      }
    } catch (cause) {
      if (token === generation.current) setError(aiError(cause));
    } finally {
      if (token === generation.current) {
        inFlight.current = false;
        setBusy(false);
        setAction(null);
      }
    }
  }
  const windows = /Win/.test(navigator.platform);
  const reason = status
    ? ((
        {
          shell: "未找到可用的命令解释器",
          owner: "当前用户不是目录所有者",
          WRITE_DAC: "缺少目录授权修改权限",
          WRITE_OWNER: "缺少目录所有者权限",
        } as Record<string, string>
      )[status.operation] ?? status.operation)
    : "";
  return {
    status,
    repair,
    busy,
    error,
    result,
    description: windows
      ? "检查当前项目的 Windows 终端沙箱权限，检查不会修改目录权限。"
      : "检查当前项目的终端沙箱权限，检查不会修改目录权限。",
    checkLabel:
      action === "check" ? "正在检查…" : status ? "重新检查" : "检查权限",
    statusLabel: !repositoryId
      ? "先打开项目后检查"
      : busy
        ? action === "check"
          ? "正在检查…"
          : action === "prepare"
            ? "正在准备修复方案…"
            : "正在修复权限…"
        : error
          ? "权限检查失败"
          : status
            ? status.ready
              ? "权限检查通过"
              : "沙箱权限需要处理"
            : "尚未检查当前项目",
    statusTone: error
      ? "error"
      : busy
        ? "loading"
        : status?.ready
          ? "ready"
          : status
            ? "warning"
            : "idle",
    detail:
      status && !status.ready
        ? `${reason}${windows && status.win32Code !== null ? `（Win32 ${status.win32Code}）` : ""}`
        : "",
    displayRoot:
      status?.root.replace(/^\\\\\?\\UNC\\/, "\\\\").replace(/^\\\\\?\\/, "") ??
      "",
    check: () => {
      void run("check");
    },
    prepare: () => {
      void run("prepare");
    },
    execute: () => {
      void run("execute");
    },
    cancel: () => setRepair(null),
  };
}
