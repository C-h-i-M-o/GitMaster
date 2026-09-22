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
  "INVALID_INPUT",
  "EMPTY_SELECTION",
  "STALE_WRITE_PLAN",
  "STALE_GRAPH",
  "STALE_CONFLICT",
  "OPERATION_IN_PROGRESS",
  "QUEUE_FULL",
  "INDEX_LOCKED",
  "UNSUPPORTED_WRITE_CONFIGURATION",
  "WORKTREE_DIRTY",
  "CONFLICT_PRESENT",
  "REPOSITORY_OPERATION_ACTIVE",
  "NOTHING_TO_COMMIT",
  "IDENTITY_REQUIRED",
  "DETACHED_HEAD_WRITE_BLOCKED",
  "HEAD_REQUIRED",
  "INVALID_BRANCH_NAME",
  "BRANCH_EXISTS",
  "BRANCH_IN_USE",
  "REMOTE_NOT_FOUND",
  "UNSUPPORTED_TRANSPORT",
  "UNTRUSTED_AUTH_CONFIGURATION",
  "AUTH_REQUIRED",
  "NETWORK_FAILED",
  "REMOTE_REJECTED",
  "NON_FAST_FORWARD",
  "REMOTE_CHANGED",
  "NO_COMMON_ANCESTOR",
  "TARGET_EXISTS",
  "CHECKOUT_UNSUPPORTED",
  "UNSUPPORTED_CONFLICT",
  "UNRESOLVED_CONFLICTS",
  "WRITE_OUTCOME_UNKNOWN",
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
