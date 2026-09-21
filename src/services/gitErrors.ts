import type { OperationError } from "../types/git";
const codes = new Set([
  "GIT_NOT_FOUND",
  "GIT_PATH_INVALID",
  "GIT_UNSUPPORTED",
  "GIT_EXECUTION_FAILED",
  "NOT_REPOSITORY",
  "BARE_REPOSITORY",
  "ACCESS_DENIED",
  "UNSAFE_REPOSITORY",
  "UNSUPPORTED_PATH_ENCODING",
  "TIMEOUT",
  "OUTPUT_LIMIT",
  "PARSE_FAILED",
  "STALE_REQUEST",
  "FILE_UNAVAILABLE",
  "SETTINGS_IO",
]);
/** 只接受契约错误码，未知异常不向界面泄露原始内容。 */
export function normalizeOperationError(value: unknown): OperationError {
  if (
    typeof value === "object" &&
    value !== null &&
    "code" in value &&
    typeof value.code === "string" &&
    codes.has(value.code)
  ) {
    return {
      code: value.code,
      retryable: "retryable" in value && value.retryable === true,
    };
  }
  return { code: "GIT_EXECUTION_FAILED", retryable: true };
}
