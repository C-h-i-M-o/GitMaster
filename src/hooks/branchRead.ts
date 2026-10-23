import type { BranchList } from "../types/git.ts";
import { normalizeOperationError } from "../services/gitErrors.ts";

/** 等待写屏障释放的短暂间隔，生命周期结束立即清理计时器。 */
function waitForRetry(delay: number, signal: AbortSignal): Promise<void> {
  return new Promise((resolve, reject) => {
    /** 清除等待并拒绝旧仓库的后续尝试。 */
    function cancel(): void {
      clearTimeout(timer);
      signal.removeEventListener("abort", cancel);
      reject({ code: "REQUEST_CANCELLED" });
    }
    const timer = setTimeout(() => {
      signal.removeEventListener("abort", cancel);
      resolve();
    }, delay);
    signal.addEventListener("abort", cancel, { once: true });
    if (signal.aborted) cancel();
  });
}

/** 仅重试写屏障取消，最多三次；真实失败原样交给恢复入口。 */
export async function readBranchesWithRetry(
  read: () => Promise<BranchList>,
  signal: AbortSignal,
): Promise<BranchList> {
  for (let attempt = 0; ; attempt++) {
    if (signal.aborted) throw { code: "REQUEST_CANCELLED" };
    try {
      const result = await read();
      if (signal.aborted) throw { code: "REQUEST_CANCELLED" };
      return result;
    } catch (error: unknown) {
      if (
        signal.aborted ||
        normalizeOperationError(error).code !== "REQUEST_CANCELLED" ||
        attempt >= 2
      )
        throw error;
      await waitForRetry(250 * (attempt + 1), signal);
    }
  }
}
