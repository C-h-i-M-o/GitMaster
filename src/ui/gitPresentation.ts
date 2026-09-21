import type { FileChange, OperationError } from "../types/git";
/** 将稳定错误码映射为面向用户的中文提示。 */
export function describeGitError(error: OperationError): string {
  const map: Record<string, string> = {
    GIT_NOT_FOUND: "没有找到 Git，请选择已安装的 Git 或打开官网。",
    GIT_PATH_INVALID: "所选路径不是可用的 Git。",
    GIT_UNSUPPORTED: "Git 版本过低。",
    GIT_EXECUTION_FAILED: "Git 执行失败，请重试。",
    NOT_REPOSITORY: "这里不是 Git 仓库。",
    BARE_REPOSITORY: "这是 bare 仓库，暂不支持。",
    ACCESS_DENIED: "没有权限读取此位置。",
    UNSAFE_REPOSITORY: "Git 拒绝读取不受信任的仓库。",
    UNSUPPORTED_PATH_ENCODING: "文件路径编码暂不支持。",
    TIMEOUT: "Git 响应超时。",
    OUTPUT_LIMIT: "内容过大，已停止读取。",
    PARSE_FAILED: "Git 返回的数据无法解析。",
    STALE_REQUEST: "内容已变化，请重新刷新。",
    FILE_UNAVAILABLE: "文件已不可用，请刷新仓库。",
    SETTINGS_IO: "应用设置读取失败。",
  };
  return map[error.code] ?? "读取失败，请重试。";
}
export function describeStatus(status: string): string {
  return (
    (
      {
        ".": "无",
        M: "修改",
        A: "新增",
        D: "删除",
        R: "重命名",
        C: "复制",
        U: "冲突",
        T: "类型变化",
        "?": "未跟踪",
      } as Record<string, string>
    )[status] ?? "未知状态"
  );
}
export function describeOperation(operation: string): string {
  return (
    (
      {
        merge: "合并中",
        rebase: "变基中",
        cherryPick: "拣选中",
        revert: "撤销中",
        bisect: "二分查找中",
      } as Record<string, string>
    )[operation] ?? operation
  );
}
export function describeUnsupported(reason: string): string {
  return (
    (
      {
        conflict: "冲突文件",
        submodule: "子模块",
        encoding: "编码不支持",
        symlink: "符号链接",
      } as Record<string, string>
    )[reason] ?? "暂不支持"
  );
}
/** 返回文件状态分组，保留同一文件在多个比较层出现。 */
export function groupChanges(
  changes: FileChange[],
): Record<"staged" | "unstaged" | "untracked" | "conflicted", FileChange[]> {
  return {
    staged: changes.filter(
      (c) =>
        c.indexStatus !== "." &&
        c.kind !== "untracked" &&
        c.kind !== "conflicted",
    ),
    unstaged: changes.filter(
      (c) =>
        c.worktreeStatus !== "." &&
        c.kind !== "untracked" &&
        c.kind !== "conflicted",
    ),
    untracked: changes.filter((c) => c.kind === "untracked"),
    conflicted: changes.filter((c) => c.kind === "conflicted"),
  };
}
export function describeHead(kind: string, name?: string): string {
  return kind === "branch"
    ? `分支 ${name ?? ""}`
    : kind === "unborn"
      ? `尚无提交 · ${name ?? ""}`
      : "分离头指针";
}
