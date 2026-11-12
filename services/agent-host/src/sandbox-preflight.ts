import type { AiPermission } from "./contracts.ts";

/** 原生只读预检结果，不包含修改 ACL 的能力或任意命令参数。 */
export interface SandboxStatus {
  ready: boolean;
  code: string;
  win32Code: number | null;
  operation: string;
}

/** 缓存会话预检，修复或重新检查时清空；执行器仍逐次约束权限。 */
export class SandboxPreflight {
  private readonly failures = new Map<string, SandboxStatus>();
  private readonly inspect: (sessionId: string) => Promise<SandboxStatus>;
  /** 检查能力只能由宿主桥接注入。 */
  constructor(inspect: (sessionId: string) => Promise<SandboxStatus>) {
    this.inspect = inspect;
  }
  /** 完全访问沿用原权限行为；受限模式必须在命令启动前通过预检。 */
  async check(sessionId: string, permission: AiPermission): Promise<void> {
    if (permission === "danger-full-access") return;
    const status =
      this.failures.get(sessionId) ?? (await this.inspect(sessionId));
    this.failures.set(sessionId, status);
    // 只读后端不向工作区写入常驻授权，因此不能要求工作区具备 WRITE_OWNER。
    if (
      permission === "read-only" &&
      status.code === "AI_SANDBOX_ACL" &&
      ["WRITE_OWNER", "WRITE_DAC", "owner"].includes(status.operation)
    )
      return;
    if (status.ready) return;
    this.failures.set(sessionId, status);
    throw new Error(
      `AI 终端沙箱不可用：stage=sandbox-preflight code=${status.code} win32Code=${status.win32Code ?? "unknown"} operation=${status.operation} commandStarted=false retryable=false。请在通用 → AI 终端检查当前项目权限。`,
    );
  }
  /** 用户重新检查、修复或调整权限后允许再次读取真实状态。 */
  invalidate(): void {
    this.failures.clear();
  }
  /** 后端初始化仍可能因子目录 ACL 失败；只缓存已确认的初始化错误，不缓存命令退出。 */
  rememberFailure(sessionId: string, detail: string): void {
    const match =
      /SetNamedSecurityInfoW failed \(win32 (\d+)\): (grantWrite|grantRead)/.exec(
        detail,
      );
    if (match)
      this.failures.set(sessionId, {
        ready: false,
        code: "AI_SANDBOX_ACL",
        win32Code: Number(match[1]),
        operation: match[2]!,
      });
  }
}
