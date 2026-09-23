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
    SETTINGS_IO: "应用设置读取或保存失败。",
    INVALID_INPUT: "输入不符合要求。",
    EMPTY_SELECTION: "请至少选择一项。",
    STALE_WRITE_PLAN: "写入预览已过期，请重新准备。",
    STALE_GRAPH: "历史已变化，请重新加载。",
    STALE_CONFLICT: "冲突内容已变化，请重新加载。",
    OPERATION_IN_PROGRESS: "已有操作正在进行。",
    QUEUE_FULL: "操作队列已满，请稍后重试。",
    WORKTREE_DIRTY: "工作区有未保存修改。",
    INDEX_LOCKED: "索引正被其他 Git 进程锁定，请等待该进程结束后重试。",
    HEAD_REQUIRED: "当前仓库还没有提交，请先创建第一个版本。",
    DETACHED_HEAD_WRITE_BLOCKED:
      "当前为分离头指针，请创建并切换到具名分支后再写入。",
    REPOSITORY_OPERATION_ACTIVE:
      "仓库存在进行中的 Git 操作，请先在原工具中完成。",
    UNSUPPORTED_WRITE_CONFIGURATION:
      "当前平台或仓库配置不支持安全写入，请在外部 Git 工具中处理。",
    UNTRUSTED_AUTH_CONFIGURATION:
      "认证配置来源无法确认，请在用户级或系统级配置可信凭据助手或 SSH agent。",
    CONFLICT_PRESENT: "仓库存在未解决冲突。",
    NOTHING_TO_COMMIT: "没有可提交的内容。",
    IDENTITY_REQUIRED: "请先配置 Git 用户身份。",
    INVALID_BRANCH_NAME: "分支名称无效。",
    BRANCH_EXISTS: "分支已存在。",
    BRANCH_IN_USE: "分支正被其他工作树使用。",
    REMOTE_NOT_FOUND: "找不到目标远端。",
    UNSUPPORTED_TRANSPORT: "远端传输协议不受支持。",
    AUTH_REQUIRED: "远端认证未完成，请检查系统凭据。",
    NETWORK_FAILED: "网络操作失败，请检查连接。",
    REMOTE_REJECTED: "远端拒绝了此次操作。",
    NON_FAST_FORWARD: "远端不允许非快进更新。",
    REMOTE_CHANGED: "远端状态已变化，请重新准备。",
    NO_COMMON_ANCESTOR: "本地与远端没有共同祖先。",
    TARGET_EXISTS: "目标目录或分支已存在。",
    CHECKOUT_UNSUPPORTED: "目标内容无法安全检出。",
    UNSUPPORTED_CONFLICT: "该冲突类型暂不支持内置处理。",
    UNRESOLVED_CONFLICTS: "仍有未解决冲突。",
    WRITE_OUTCOME_UNKNOWN: "写入结果未知，请重新读取状态。",
  };
  const message =
    error.code === "GIT_EXECUTION_FAILED" &&
    error.diagnostic?.stage === "checkoutInit"
      ? "分支切换准备失败：临时仓库初始化失败"
      : (map[error.code] ?? "操作未完成，请刷新状态后重试。");
  const diagnostic = error.diagnostic;
  if (!diagnostic) return message;
  const stages: Record<string, string> = {
    revParse: "解析仓库",
    status: "读取状态",
    log: "读取历史",
    revList: "读取提交关系",
    forEachRef: "读取引用",
    gitQuery: "查询 Git",
    windowsProcess: "启动 Windows 进程",
    checkoutInit: "初始化临时仓库",
  };
  const details = [`阶段：${stages[diagnostic.stage] ?? "Git 操作"}`];
  if (diagnostic.osCode !== undefined)
    details.push(`系统码：${diagnostic.osCode}`);
  if (diagnostic.exitCode !== undefined)
    details.push(`退出码：${diagnostic.exitCode}`);
  return `${message}（${details.join("，")}）`;
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
