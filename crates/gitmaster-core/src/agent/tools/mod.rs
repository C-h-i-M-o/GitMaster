//! 固定能力的工具执行器；模型不能传入 shell 或任意 Git 参数。
pub mod files;
pub mod git;
pub mod network;
pub mod registry;
use super::*;

/// 保留可诊断的错误码，展示中文说明，不暴露 Git stderr。
pub(crate) fn git_error(error: crate::git::OperationError) -> AgentError {
    let message = match error.code.as_str() {
        "FILE_CHANGED" | "STALE_REQUEST" | "STALE_WRITE_PLAN" => {
            "目标已变化，请重新读取并确认操作。"
        }
        "FILE_EDIT_BINARY" | "FILE_EDIT_ENCODING" => "该文件不是可编辑的 UTF-8 文本。",
        "OUTPUT_LIMIT" | "FILE_EDIT_TOO_LARGE" => "文件或输出过大，未读取不完整内容用于编辑。",
        "ACCESS_DENIED" | "FILE_UNAVAILABLE" => "无法访问目标，请检查路径、权限和文件类型。",
        "WORKTREE_DIRTY" => "工作区有未提交修改，当前操作不能继续。",
        "OPERATION_IN_PROGRESS" | "QUEUE_FULL" => "仓库正在执行其他操作，请稍后重试。",
        "AUTH_REQUIRED" => "Git 远端需要认证，请先在系统 Git 中完成认证。",
        "TIMEOUT" => "Git 操作超时，请检查仓库和网络状态。",
        _ => "操作未完成，请检查仓库状态与工具参数。",
    };
    AgentError::new(&error.code, message)
}
