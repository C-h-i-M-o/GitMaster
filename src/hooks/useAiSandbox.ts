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
  useEffect(() => {
    generation.current++;
    setBusy(false);
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
    if (!repositoryId || busy) return;
    const token = ++generation.current;
    setBusy(true);
    setError("");
    try {
      if (action === "check") {
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
      if (token === generation.current) setBusy(false);
    }
  }
  return {
    status,
    repair,
    busy,
    error,
    result,
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
